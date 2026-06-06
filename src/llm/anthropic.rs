use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};

use crate::errors::{AiRestError, ErrorType};
use crate::llm::credentials::ProviderCredentials;
use crate::llm::tools::{ChatMessage as InternalMessage, ToolCall, TokenUsage};
use crate::llm::types::{LlmRequest, LlmResponse};

pub async fn complete_anthropic(
    client: &Client,
    creds: &ProviderCredentials,
    request: &LlmRequest,
) -> Result<LlmResponse, AiRestError> {
    let started = std::time::Instant::now();
    let url = format!("{}/v1/messages", creds.base_url.trim_end_matches('/'));
    let api_version = creds
        .api_version
        .as_deref()
        .unwrap_or("2023-06-01");
    let max_tokens = request.max_tokens.unwrap_or(4096);

    let (system, messages) = build_anthropic_messages(request);
    let tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect::<Vec<_>>()
    });

    let body = AnthropicRequest {
        model: request.model.clone(),
        max_tokens,
        temperature: request.temperature,
        system,
        messages,
        tools,
    };

    let response = client
        .post(url)
        .header("x-api-key", &creds.api_key)
        .header("anthropic-version", api_version)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| provider_error("Anthropic request failed", e.to_string()))?;

    let status = response.status();
    let response_body: Value = response.json().await.map_err(|e| {
        provider_error("Failed to parse Anthropic response", e.to_string())
    })?;

    if !status.is_success() {
        return Err(provider_error_with_body(
            "Anthropic API returned an error",
            status.as_u16(),
            response_body,
        ));
    }

    let mut content = String::new();
    let mut tool_calls = Vec::new();

    if let Some(blocks) = response_body.get("content").and_then(|v| v.as_array()) {
        for block in blocks {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        content.push_str(text);
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool_use")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let arguments = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
                _ => {}
            }
        }
    }

    if content.is_empty() && tool_calls.is_empty() {
        return Err(provider_error_with_body(
            "Anthropic response missing text and tool_use blocks",
            status.as_u16(),
            response_body.clone(),
        ));
    }

    let usage = response_body.get("usage").map(|u| TokenUsage {
        input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()),
        output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()),
    });

    Ok(LlmResponse {
        content,
        tool_calls,
        usage,
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

fn build_anthropic_messages(request: &LlmRequest) -> (String, Vec<AnthropicMessage>) {
    let source = request.messages.clone().unwrap_or_else(|| {
        InternalMessage::initial_conversation(&request.system_prompt, &request.user_prompt)
    });

    let mut system = request.system_prompt.clone();
    let mut messages = Vec::new();

    for msg in source {
        match msg {
            InternalMessage::System { content } => system = content,
            InternalMessage::User { content } => {
                messages.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::Text { text: content }],
                });
            }
            InternalMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut blocks = Vec::new();
                if let Some(text) = content.filter(|s| !s.is_empty()) {
                    blocks.push(AnthropicContentBlock::Text { text });
                }
                for tc in tool_calls {
                    blocks.push(AnthropicContentBlock::ToolUse {
                        id: tc.id,
                        name: tc.name,
                        input: tc.arguments,
                    });
                }
                if !blocks.is_empty() {
                    messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: blocks,
                    });
                }
            }
            InternalMessage::Tool {
                tool_call_id,
                content,
            } => {
                messages.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::ToolResult {
                        tool_use_id: tool_call_id,
                        content,
                    }],
                });
            }
        }
    }

    (system, messages)
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
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    system: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}
