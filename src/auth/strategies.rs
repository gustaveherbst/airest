use async_trait::async_trait;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use serde::Deserialize;
use std::sync::{Arc, RwLock};

use crate::auth::verifier::{AuthVerifier, VerifyContext};
use crate::auth::{AuthContext, JtiDenylist};
use crate::config::Config;
use crate::definitions::AuthConfig;
use crate::errors::{AiRestError, ErrorType};

pub struct NoneVerifier;

#[async_trait]
impl AuthVerifier for NoneVerifier {
    fn strategy(&self) -> &'static str {
        "none"
    }

    async fn verify(&self, _ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError> {
        Ok(None)
    }
}

pub struct ApiKeyVerifier;

#[async_trait]
impl AuthVerifier for ApiKeyVerifier {
    fn strategy(&self) -> &'static str {
        "apiKey"
    }

    async fn verify(&self, ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError> {
        verify_api_key(ctx.config, ctx.headers)
    }
}

pub struct TrustGatewayVerifier;

#[async_trait]
impl AuthVerifier for TrustGatewayVerifier {
    fn strategy(&self) -> &'static str {
        "trustGateway"
    }

    async fn verify(&self, ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError> {
        verify_trust_gateway(ctx.auth, ctx.headers)
    }
}

pub struct JwtVerifier;

#[async_trait]
impl AuthVerifier for JwtVerifier {
    fn strategy(&self) -> &'static str {
        "jwt"
    }

    async fn verify(&self, ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError> {
        verify_jwt(
            ctx.config,
            ctx.auth,
            ctx.headers,
            ctx.jwks_cache,
            ctx.http,
            ctx.jti_denylist,
        )
        .await
    }
}

pub struct OAuth2IntrospectVerifier;

#[async_trait]
impl AuthVerifier for OAuth2IntrospectVerifier {
    fn strategy(&self) -> &'static str {
        "oauth2Introspect"
    }

    async fn verify(&self, ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError> {
        verify_oauth2_introspect(ctx.config, ctx.auth, ctx.headers, ctx.http).await
    }
}

fn verify_api_key(config: &Config, headers: &HeaderMap) -> Result<Option<AuthContext>, AiRestError> {
    let Some(expected_key) = config.api_key() else {
        return Ok(None);
    };

    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(provided_key) if provided_key == expected_key => Ok(Some(AuthContext {
            subject: Some("api-key".to_string()),
            tenant_id: None,
            scopes: vec![],
            raw_claims: None,
        })),
        _ => Err(AiRestError::new(
            ErrorType::Authentication,
            "Missing or invalid API key.",
        )),
    }
}

fn verify_trust_gateway(
    auth: &AuthConfig,
    headers: &HeaderMap,
) -> Result<Option<AuthContext>, AiRestError> {
    let cfg = auth.trust_gateway.as_ref().ok_or_else(|| {
        AiRestError::new(
            ErrorType::Authentication,
            "trustGateway auth requires trustGateway configuration.",
        )
    })?;

    let user_header = cfg
        .user_id_header
        .as_deref()
        .unwrap_or("x-user-id");
    let tenant_header = cfg
        .tenant_id_header
        .as_deref()
        .unwrap_or("x-tenant-id");

    let subject = headers
        .get(user_header)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if subject.is_none() {
        return Err(AiRestError::new(
            ErrorType::Authentication,
            "Missing trusted gateway user header.",
        ));
    }

    Ok(Some(AuthContext {
        subject,
        tenant_id: headers
            .get(tenant_header)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string),
        scopes: vec![],
        raw_claims: None,
    }))
}

async fn verify_jwt(
    config: &Config,
    auth: &AuthConfig,
    headers: &HeaderMap,
    jwks_cache: &JwksCache,
    http: &Client,
    jti_denylist: &JtiDenylist,
) -> Result<Option<AuthContext>, AiRestError> {
    let jwt_cfg = auth.jwt.as_ref().ok_or_else(|| {
        AiRestError::new(
            ErrorType::Authentication,
            "JWT auth requires jwt configuration.",
        )
    })?;

    let token = bearer_token(headers)?;
    let jwks_url = jwt_cfg
        .jwks_url
        .as_deref()
        .or(config.jwt_jwks_url())
        .ok_or_else(|| {
            AiRestError::new(
                ErrorType::Authentication,
                "JWT verification requires jwksUrl.",
            )
        })?;

    let header = decode_header(&token).map_err(|_| invalid_token())?;
    let kid = header.kid.ok_or_else(invalid_token)?;
    let key = jwks_cache.get_or_fetch(http, jwks_url, &kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    if let Some(issuer) = jwt_cfg.issuer.as_deref().or(config.jwt_issuer()) {
        validation.set_issuer(&[issuer]);
    }
    if let Some(audience) = jwt_cfg.audience.as_deref().or(config.jwt_audience()) {
        validation.set_audience(&[audience]);
    }

    let token_data = decode::<serde_json::Value>(&token, &key, &validation)
        .map_err(|_| invalid_token())?;

    if let Some(jti) = token_data.claims.get("jti").and_then(|v| v.as_str()) {
        if jti_denylist.is_denied(jti).await? {
            return Err(jti_denylist.deny_token());
        }
    }

    if let Some(claims_cfg) = &jwt_cfg.claims {
        if let Some(required) = &claims_cfg.required {
            for claim in required {
                if token_data.claims.get(claim).is_none() {
                    return Err(AiRestError::new(
                        ErrorType::Authentication,
                        format!("Missing required JWT claim: {claim}"),
                    ));
                }
            }
        }
        if let Some(scope) = &claims_cfg.scope {
            let token_scope = token_data
                .claims
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !token_scope.split_whitespace().any(|s| s == scope) {
                return Err(AiRestError::new(
                    ErrorType::Authentication,
                    "JWT scope claim does not grant access.",
                ));
            }
        }
    }

    Ok(Some(AuthContext {
        subject: token_data
            .claims
            .get("sub")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        tenant_id: token_data
            .claims
            .get("tenant")
            .or_else(|| token_data.claims.get("tid"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        scopes: token_data
            .claims
            .get("scope")
            .and_then(|v| v.as_str())
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        raw_claims: Some(token_data.claims),
    }))
}

async fn verify_oauth2_introspect(
    config: &Config,
    auth: &AuthConfig,
    headers: &HeaderMap,
    http: &Client,
) -> Result<Option<AuthContext>, AiRestError> {
    let oauth = auth.oauth2.as_ref().ok_or_else(|| {
        AiRestError::new(
            ErrorType::Authentication,
            "OAuth2 introspection requires oauth2 configuration.",
        )
    })?;

    let url = oauth
        .url
        .as_deref()
        .or(config.oauth2_introspection_url())
        .ok_or_else(|| {
            AiRestError::new(
                ErrorType::Authentication,
                "OAuth2 introspection URL is not configured.",
            )
        })?;

    let token = bearer_token(headers)?;

    #[derive(Deserialize)]
    struct IntrospectionResponse {
        active: bool,
        #[serde(default)]
        sub: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    }

    let client_id = oauth
        .client_id
        .as_deref()
        .or(config.oauth2_client_id())
        .unwrap_or("");
    let client_secret = oauth
        .client_secret
        .as_deref()
        .or(config.oauth2_client_secret())
        .unwrap_or("");

    let response = http
        .post(url)
        .basic_auth(client_id, Some(client_secret))
        .form(&[("token", token.as_str())])
        .send()
        .await
        .map_err(|_| invalid_token())?;

    let body: IntrospectionResponse = response.json().await.map_err(|_| invalid_token())?;
    if !body.active {
        return Err(invalid_token());
    }

    Ok(Some(AuthContext {
        subject: body.sub,
        tenant_id: None,
        scopes: body
            .scope
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        raw_claims: None,
    }))
}

fn bearer_token(headers: &HeaderMap) -> Result<String, AiRestError> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(invalid_token)?;

    value
        .strip_prefix("Bearer ")
        .map(str::to_string)
        .ok_or_else(invalid_token)
}

fn invalid_token() -> AiRestError {
    AiRestError::new(ErrorType::Authentication, "Missing or invalid bearer token.")
}

#[derive(Clone, Default)]
pub struct JwksCache {
    keys: Arc<RwLock<std::collections::HashMap<String, DecodingKey>>>,
}

impl JwksCache {
    pub async fn get_or_fetch(
        &self,
        http: &Client,
        jwks_url: &str,
        kid: &str,
    ) -> Result<DecodingKey, AiRestError> {
        if let Some(key) = self.keys.read().ok().and_then(|g| g.get(kid).cloned()) {
            return Ok(key);
        }

        #[derive(Deserialize)]
        struct Jwks {
            keys: Vec<serde_json::Value>,
        }

        let jwks: Jwks = http
            .get(jwks_url)
            .send()
            .await
            .map_err(|_| invalid_token())?
            .json()
            .await
            .map_err(|_| invalid_token())?;

        let key_json = jwks
            .keys
            .into_iter()
            .find(|k| k.get("kid").and_then(|v| v.as_str()) == Some(kid))
            .ok_or_else(invalid_token)?;

        let key = DecodingKey::from_rsa_components(
            key_json["n"].as_str().ok_or_else(invalid_token)?,
            key_json["e"].as_str().ok_or_else(invalid_token)?,
        )
        .map_err(|_| invalid_token())?;

        if let Ok(mut guard) = self.keys.write() {
            guard.insert(kid.to_string(), key.clone());
        }

        Ok(key)
    }
}
