use sha2::{Digest, Sha256};

use crate::definitions::EndpointDefinition;

/// Stable fingerprint of everything that affects cached LLM output for an endpoint.
pub fn endpoint_fingerprint(endpoint: &EndpointDefinition) -> String {
    let mut hasher = Sha256::new();
    hasher.update(endpoint.name.as_bytes());
    hasher.update(endpoint.version.as_bytes());
    hasher.update(endpoint.model.provider.as_bytes());
    hasher.update(endpoint.model.model.as_bytes());
    hasher.update(endpoint.system_prompt.as_bytes());
    if let Some(template) = &endpoint.user_prompt_template {
        hasher.update(template.as_bytes());
    }
    format!("{:x}", hasher.finalize())[..16].to_string()
}
