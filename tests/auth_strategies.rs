use airest::auth::{verify_request, JtiDenylist, JwksCache};
use airest::config::Config;
use airest::definitions::{minimal_test_endpoint, AuthConfig, OAuth2IntrospectConfig, TrustGatewayConfig};
use axum::http::HeaderMap;
use reqwest::Client;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn endpoint_with_auth(auth: AuthConfig) -> airest::definitions::EndpointDefinition {
    let mut endpoint = minimal_test_endpoint();
    endpoint.auth = Some(auth);
    endpoint
}

#[tokio::test]
async fn oauth2_introspection_accepts_active_token() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": true,
            "sub": "user-123",
            "scope": "read write"
        })))
        .mount(&mock)
        .await;

    let mut config = Config::for_test(None, std::path::PathBuf::from("api"));
    config.oauth2_introspection_url = Some(format!("{}/introspect", mock.uri()));
    config.oauth2_client_id = Some("client".to_string());
    config.oauth2_client_secret = Some("secret".to_string());

    let endpoint = endpoint_with_auth(AuthConfig {
            required: true,
            r#type: Some("oauth2Introspect".to_string()),
            jwt: None,
            oauth2: Some(OAuth2IntrospectConfig {
                url: Some(format!("{}/introspect", mock.uri())),
                client_id: Some("client".to_string()),
                client_secret: Some("secret".to_string()),
            }),
            trust_gateway: None,
    });

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        "Bearer good-token".parse().unwrap(),
    );

    let ctx = verify_request(
        &config,
        &endpoint,
        &headers,
        &JwksCache::default(),
        &Client::new(),
        &JtiDenylist::default(),
    )
    .await
    .expect("oauth2 introspection should succeed");

    assert_eq!(ctx.and_then(|c| c.subject), Some("user-123".to_string()));
}

#[tokio::test]
async fn oauth2_introspection_rejects_inactive_token() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/introspect"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "active": false
        })))
        .mount(&mock)
        .await;

    let mut config = Config::for_test(None, std::path::PathBuf::from("api"));
    config.oauth2_introspection_url = Some(format!("{}/introspect", mock.uri()));
    config.oauth2_client_id = Some("client".to_string());
    config.oauth2_client_secret = Some("secret".to_string());

    let endpoint = endpoint_with_auth(AuthConfig {
            required: true,
            r#type: Some("oauth2Introspect".to_string()),
            jwt: None,
            oauth2: Some(OAuth2IntrospectConfig::default()),
            trust_gateway: None,
    });

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        "Bearer revoked-token".parse().unwrap(),
    );

    let err = verify_request(
        &config,
        &endpoint,
        &headers,
        &JwksCache::default(),
        &Client::new(),
        &JtiDenylist::default(),
    )
    .await
    .expect_err("inactive token should fail");

    assert_eq!(err.error_type(), airest::errors::ErrorType::Authentication);
}

#[tokio::test]
async fn trust_gateway_reads_identity_headers() {
    let config = Config::for_test(None, std::path::PathBuf::from("api"));
    let endpoint = endpoint_with_auth(AuthConfig {
            required: true,
            r#type: Some("trustGateway".to_string()),
            jwt: None,
            oauth2: None,
            trust_gateway: Some(TrustGatewayConfig {
                user_id_header: Some("x-user-id".to_string()),
                tenant_id_header: Some("x-tenant-id".to_string()),
            }),
    });

    let mut headers = HeaderMap::new();
    headers.insert("x-user-id", "alice".parse().unwrap());
    headers.insert("x-tenant-id", "tenant-9".parse().unwrap());

    let ctx = verify_request(
        &config,
        &endpoint,
        &headers,
        &JwksCache::default(),
        &Client::new(),
        &JtiDenylist::default(),
    )
    .await
    .expect("trust gateway auth");

    let ctx = ctx.expect("auth context");
    assert_eq!(ctx.subject.as_deref(), Some("alice"));
    assert_eq!(ctx.tenant_id.as_deref(), Some("tenant-9"));
}

#[tokio::test]
async fn static_jti_denylist_rejects_revoked_tokens() {
    let mut config = Config::for_test(None, std::path::PathBuf::from("api"));
    config.jti_denylist = vec!["revoked-jti".to_string()];
    let denylist = JtiDenylist::from_config(&config).await;

    assert!(denylist.is_denied("revoked-jti").await.unwrap());
    assert!(!denylist.is_denied("valid-jti").await.unwrap());
}
