use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{AiRestError, ErrorType};

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub output_schema: Value,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub latency_ms: u64,
}

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
        }
    }

    pub async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, AiRestError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let started = std::time::Instant::now();

        let body = ChatCompletionRequest {
            model: request.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: request.system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: request.user_prompt,
                },
            ],
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                AiRestError::with_details(
                    ErrorType::ModelProvider,
                    "Failed to call OpenAI-compatible API",
                    serde_json::json!({ "reason": e.to_string() }),
                )
            })?;

        let status = response.status();
        let response_body: Value = response.json().await.map_err(|e| {
            AiRestError::with_details(
                ErrorType::ModelProvider,
                "Failed to parse OpenAI API response",
                serde_json::json!({ "reason": e.to_string() }),
            )
        })?;

        if !status.is_success() {
            return Err(AiRestError::with_details(
                ErrorType::ModelProvider,
                "OpenAI-compatible API returned an error",
                serde_json::json!({
                    "status": status.as_u16(),
                    "body": response_body,
                }),
            ));
        }

        let content = response_body
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AiRestError::with_details(
                    ErrorType::ModelProvider,
                    "OpenAI response missing message content",
                    response_body.clone(),
                )
            })?
            .to_string();

        Ok(LlmResponse {
            content,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Choice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatMessageResponse {
    content: String,
}
