pub mod typescript;

use std::path::Path;

use crate::errors::AiRestError;

/// Prepare a Deno sandbox script for execution (TypeScript → JavaScript when needed).
pub fn prepare_deno_script(source: &str, path: Option<&Path>) -> Result<String, AiRestError> {
    let is_typescript = path
        .and_then(|p| p.extension())
        .is_some_and(|ext| ext == "ts" || ext == "tsx")
        || looks_like_typescript(source);

    if is_typescript {
        typescript::transpile_guardrail_script(source)
    } else {
        Ok(source.to_string())
    }
}

fn looks_like_typescript(source: &str) -> bool {
    source.contains(": GuardrailEvaluate")
        || source.contains("interface ")
        || source.contains("type Guardrail")
        || source.contains(": string")
        || source.contains(": number")
        || source.contains(" as string")
}
