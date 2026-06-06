use handlebars::Handlebars;
use serde_json::Value;

use crate::errors::{AiRestError, ErrorType};

pub struct RenderedPrompt {
    pub system: String,
    pub user: String,
}

impl Clone for RenderedPrompt {
    fn clone(&self) -> Self {
        Self {
            system: self.system.clone(),
            user: self.user.clone(),
        }
    }
}

pub fn render_prompt(
    system_prompt: &str,
    user_prompt_template: Option<&str>,
    output_schema: &Value,
    input: &Value,
) -> Result<RenderedPrompt, AiRestError> {
    let user_body = if let Some(template) = user_prompt_template {
        render_template(template, input)?
    } else {
        serde_json::to_string_pretty(input).map_err(|e| {
            AiRestError::new(
                ErrorType::PromptRendering,
                format!("Failed to serialize input for prompt: {e}"),
            )
        })?
    };

    let schema_instruction = build_schema_instruction(output_schema);
    let user = format!("{user_body}\n\n{schema_instruction}");

    Ok(RenderedPrompt {
        system: system_prompt.to_string(),
        user,
    })
}

pub fn render_correction_prompt(output_schema: &Value, validation_errors: &[String]) -> String {
    let errors = validation_errors.join("\n");
    format!(
        "Your previous response did not match the required output schema.\n\n\
         Validation errors:\n{errors}\n\n\
         Return a corrected response as valid JSON only.\n\n\
         Required JSON Schema:\n{}",
        serde_json::to_string_pretty(output_schema).unwrap_or_else(|_| "{}".to_string())
    )
}

fn render_template(template: &str, input: &Value) -> Result<String, AiRestError> {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(false);

    let input_map: serde_json::Map<String, Value> = input
        .as_object()
        .cloned()
        .ok_or_else(|| {
            AiRestError::new(
                ErrorType::PromptRendering,
                "Input must be a JSON object for template rendering",
            )
        })?;

    handlebars
        .render_template(template, &input_map)
        .map_err(|e| {
            AiRestError::with_details(
                ErrorType::PromptRendering,
                "Failed to render user prompt template",
                serde_json::json!({ "reason": e.to_string() }),
            )
        })
}

fn build_schema_instruction(output_schema: &Value) -> String {
    let schema_json =
        serde_json::to_string_pretty(output_schema).unwrap_or_else(|_| "{}".to_string());

    format!(
        "You must return only valid JSON.\n\n\
         Do not include markdown.\n\
         Do not include code fences.\n\
         Do not include explanations outside the JSON.\n\
         The JSON must conform exactly to this JSON Schema:\n\n\
         {schema_json}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_template_with_input_fields() {
        let template = "Text:\n{{text}}";
        let input = json!({ "text": "hello world" });
        let output_schema = json!({ "type": "object" });

        let rendered = render_prompt("system", Some(template), &output_schema, &input).unwrap();

        assert!(rendered.user.contains("hello world"));
        assert!(rendered.user.contains("You must return only valid JSON"));
    }

    #[test]
    fn renders_missing_optional_template_variables_as_empty() {
        let template = "Text:\n{{text}}\nTier:\n{{customerTier}}";
        let input = json!({ "text": "hello" });
        let output_schema = json!({ "type": "object" });

        let rendered = render_prompt("system", Some(template), &output_schema, &input).unwrap();
        assert!(rendered.user.contains("hello"));
        assert!(rendered.user.contains("Tier:"));
    }
}
