use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::definitions::EndpointDefinition;
use crate::llm::{ProviderConfig, ProviderKind};

const PLACEHOLDER_SECRETS: &[&str] = &[
    "replace-me",
    "changeme",
    "change-me",
    "your-api-key-here",
    "xxx",
    "test-key",
];

pub fn validate_production_config(
    config: &Config,
    providers: &ProviderConfig,
    endpoints: &[EndpointDefinition],
) -> Result<()> {
    if !config.production_mode {
        return Ok(());
    }

    if let Some(api_key) = config.api_key() {
        if is_placeholder_secret(api_key) {
            bail!("AIREST_API_KEY must not be a placeholder when AIREST_PRODUCTION=true");
        }
    } else if endpoints.iter().any(|endpoint| {
        endpoint
            .auth
            .as_ref()
            .map(|auth| auth.required && auth.auth_type() == "apiKey")
            .unwrap_or(false)
    }) {
        bail!(
            "AIREST_PRODUCTION=true requires AIREST_API_KEY when endpoints use apiKey auth"
        );
    }

    let used: Vec<ProviderKind> = endpoints
        .iter()
        .map(|endpoint| {
            endpoint
                .model
                .provider_kind()
                .with_context(|| format!("Invalid provider in endpoint '{}'", endpoint.name))
        })
        .collect::<Result<Vec<_>>>()?;

    for provider in used {
        let creds = providers
            .credentials_for(provider)
            .with_context(|| format!("Provider '{}' is not configured", provider.as_str()))?;
        if provider != ProviderKind::Ollama && is_placeholder_secret(&creds.api_key) {
            bail!(
                "Provider '{}' API key must not be a placeholder when AIREST_PRODUCTION=true",
                provider.as_str()
            );
        }
    }

    if config.hot_reload {
        bail!("AIREST_HOT_RELOAD must be disabled when AIREST_PRODUCTION=true");
    }

    Ok(())
}

fn is_placeholder_secret(value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    PLACEHOLDER_SECRETS
        .iter()
        .any(|placeholder| normalized == *placeholder)
}
