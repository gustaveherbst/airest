use std::path::PathBuf;

use super::types::EndpointDefinition;

/// Endpoint definition plus its source file path (for relative guardrail script paths).
#[derive(Debug, Clone)]
pub struct LoadedEndpoint {
    pub definition: EndpointDefinition,
    pub source_path: PathBuf,
}
