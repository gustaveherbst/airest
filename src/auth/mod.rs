mod context;
mod denylist;
mod registry;
mod strategies;
mod verifier;
mod verifiers;

pub use context::AuthContext;
pub use denylist::JtiDenylist;
pub use registry::AuthRegistry;
pub use verifier::{AuthVerifier, VerifyContext};
pub use verifiers::{verify_request, JwksCache};

use axum::http::HeaderMap;

use crate::config::Config;
use crate::definitions::EndpointDefinition;
use crate::errors::{AiRestError, ErrorType};

/// Legacy helper — prefer `verify_request` with full auth strategies.
pub fn verify_api_key(
    config: &Config,
    endpoint: &EndpointDefinition,
    headers: &HeaderMap,
) -> Result<(), AiRestError> {
    if !endpoint.auth_required() {
        return Ok(());
    }

    let Some(expected_key) = config.api_key() else {
        return Ok(());
    };

    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(provided_key) if provided_key == expected_key => Ok(()),
        _ => Err(AiRestError::new(
            ErrorType::Authentication,
            "Missing or invalid API key.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{minimal_test_endpoint, AuthConfig};

    fn endpoint_with_auth(required: bool, auth_type: &str) -> EndpointDefinition {
        let mut endpoint = minimal_test_endpoint();
        endpoint.auth = Some(AuthConfig {
            required,
            r#type: Some(auth_type.to_string()),
            jwt: None,
            oauth2: None,
            trust_gateway: None,
        });
        endpoint
    }

    #[test]
    fn skips_auth_when_server_key_blank() {
        let mut config = Config::from_env().expect("config");
        config.api_key = None;
        let endpoint = endpoint_with_auth(true, "apiKey");
        let headers = HeaderMap::new();
        assert!(verify_api_key(&config, &endpoint, &headers).is_ok());
    }
}
