use jsonschema::{Draft, Validator};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("schema compilation failed: {0}")]
    SchemaCompilation(String),
    #[error("validation failed")]
    ValidationFailed { errors: Vec<String> },
}

pub fn validate_input(schema: &Value, data: &Value) -> Result<(), ValidationError> {
    validate_against_schema(schema, data)
}

pub fn validate_output(schema: &Value, data: &Value) -> Result<(), ValidationError> {
    validate_against_schema(schema, data)
}

fn validate_against_schema(schema: &Value, data: &Value) -> Result<(), ValidationError> {
    let compiled = Validator::options()
        .with_draft(Draft::Draft7)
        .build(schema)
        .map_err(|e| ValidationError::SchemaCompilation(e.to_string()))?;

    let errors: Vec<String> = compiled
        .iter_errors(data)
        .map(|e| e.to_string())
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::ValidationFailed { errors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_input_successfully() {
        let schema = json!({
            "type": "object",
            "required": ["text"],
            "properties": { "text": { "type": "string", "minLength": 1 } }
        });
        let data = json!({ "text": "hello" });
        assert!(validate_input(&schema, &data).is_ok());
    }

    #[test]
    fn rejects_invalid_input() {
        let schema = json!({
            "type": "object",
            "required": ["text"],
            "properties": { "text": { "type": "string", "minLength": 1 } }
        });
        let data = json!({});
        let err = validate_input(&schema, &data).unwrap_err();
        match err {
            ValidationError::ValidationFailed { errors } => assert!(!errors.is_empty()),
            _ => panic!("expected validation failed"),
        }
    }
}
