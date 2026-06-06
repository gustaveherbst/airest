use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::errors::{AiRestError, ErrorType};
use crate::llm::credentials::ProviderCredentials;
use crate::llm::types::{LlmRequest, LlmResponse};

pub async fn complete_gemini(
    client: &Client,
    creds: &ProviderCredentials,
    request: &LlmRequest,
) -> Result<LlmResponse, AiRestError> {
    let started = std::time::Instant::now();
    let url = format!(
        "{}/models/{}:generateContent",
        creds.base_url.trim_end_matches('/'),
        request.model
    );

    let body = GeminiRequest {
        system_instruction: Some(GeminiContent {
            parts: vec![GeminiPart {
                text: request.system_prompt.clone(),
            }],
        }),
        contents: vec![GeminiContent {
            parts: vec![GeminiPart {
                text: request.user_prompt.clone(),
            }],
        }],
        generation_config: Some(GeminiGenerationConfig {
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            response_mime_type: Some("application/json".to_string()),
        }),
    };

    let response = client
        .post(url)
        .query(&[("key", creds.api_key.as_str())])
        .json(&body)
        .send()
        .await
        .map_err(|e| provider_error("Gemini request failed", e.to_string()))?;

    let status = response.status();
    let response_body: Value = response.json().await.map_err(|e| {
        provider_error("Failed to parse Gemini response", e.to_string())
    })?;

    if !status.is_success() {
        return Err(provider_error_with_body(
            "Gemini API returned an error",
            status.as_u16(),
            response_body,
        ));
    }

    let content = response_body
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            provider_error_with_body(
                "Gemini response missing text content",
                status.as_u16(),
                response_body.clone(),
            )
        })?
        .to_string();

    Ok(LlmResponse {
        content,
        tool_calls: Vec::new(),
        usage: None,
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

fn provider_error(message: &str, reason: String) -> AiRestError {
    AiRestError::with_details(
        ErrorType::ModelProvider,
        message,
        serde_json::json!({ "reason": reason }),
    )
}

fn provider_error_with_body(message: &str, status: u16, body: Value) -> AiRestError {
    AiRestError::with_details(
        ErrorType::ModelProvider,
        message,
        serde_json::json!({ "status": status, "body": body }),
    )
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
}
