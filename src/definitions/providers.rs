use std::collections::HashSet;

use anyhow::{Context, Result};

use crate::definitions::EndpointDefinition;
use crate::llm::{ProviderConfig, ProviderKind};

pub fn validate_provider_credentials(
    providers: &ProviderConfig,
    endpoints: &[EndpointDefinition],
) -> Result<()> {
    let used: HashSet<ProviderKind> = endpoints
        .iter()
        .map(|endpoint| {
            endpoint
                .model
                .provider_kind()
                .with_context(|| format!("Invalid provider in endpoint '{}'", endpoint.name))
        })
        .collect::<Result<_>>()?;

    let unique: Vec<ProviderKind> = used.into_iter().collect();
    providers.validate_for_providers(&unique)
}
