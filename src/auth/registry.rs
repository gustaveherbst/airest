use std::collections::HashMap;
use std::sync::Arc;

use axum::http::HeaderMap;

use crate::auth::strategies::{
    ApiKeyVerifier, JwtVerifier, NoneVerifier, OAuth2IntrospectVerifier, TrustGatewayVerifier,
};
use crate::auth::verifier::{AuthVerifier, VerifyContext};
use crate::auth::{AuthContext, JtiDenylist, JwksCache};
use crate::config::Config;
use crate::definitions::EndpointDefinition;
use crate::errors::AiRestError;
use reqwest::Client;

pub struct AuthRegistry {
    verifiers: HashMap<&'static str, Arc<dyn AuthVerifier>>,
}

impl Default for AuthRegistry {
    fn default() -> Self {
        let mut verifiers: HashMap<&'static str, Arc<dyn AuthVerifier>> = HashMap::new();
        verifiers.insert("none", Arc::new(NoneVerifier));
        verifiers.insert("apiKey", Arc::new(ApiKeyVerifier));
        verifiers.insert("jwt", Arc::new(JwtVerifier));
        verifiers.insert("oauth2Introspect", Arc::new(OAuth2IntrospectVerifier));
        verifiers.insert("trustGateway", Arc::new(TrustGatewayVerifier));
        Self { verifiers }
    }
}

impl AuthRegistry {
    pub fn global() -> &'static Self {
        static REGISTRY: std::sync::OnceLock<AuthRegistry> = std::sync::OnceLock::new();
        REGISTRY.get_or_init(AuthRegistry::default)
    }

    pub fn get(&self, strategy: &str) -> Option<&Arc<dyn AuthVerifier>> {
        self.verifiers.get(strategy)
    }
}

pub async fn verify_request(
    config: &Config,
    endpoint: &EndpointDefinition,
    headers: &HeaderMap,
    jwks_cache: &JwksCache,
    http: &Client,
    jti_denylist: &JtiDenylist,
) -> Result<Option<AuthContext>, AiRestError> {
    let auth = endpoint.auth.as_ref();
    if auth.is_none() || !auth.as_ref().map(|a| a.required).unwrap_or(false) {
        return Ok(None);
    }

    let auth = auth.unwrap();
    let strategy = auth.auth_type();
    let registry = AuthRegistry::global();
    let verifier = registry.get(strategy).ok_or_else(|| {
        AiRestError::new(
            crate::errors::ErrorType::Authentication,
            format!("Unsupported auth type: {strategy}"),
        )
    })?;

    let ctx = VerifyContext {
        config,
        auth,
        headers,
        jwks_cache,
        http,
        jti_denylist,
    };

    verifier.verify(&ctx).await
}
