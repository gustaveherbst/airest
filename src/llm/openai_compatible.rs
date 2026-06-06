use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};

use crate::errors::{AiRestError, ErrorType};
use crate::llm::credentials::ProviderCredentials;
use crate::llm::tools::{ChatMessage as InternalMessage, ToolCall, TokenUsage};
use crate::llm::types::{LlmRequest, LlmResponse};

#[derive(Clone, Copy)]
pub(crate) enum AuthStyle {
    Bearer,
    ApiKeyHeader,
    None,
}

pub async fn complete_openai_compatible(
    client: &Client,
    creds: &ProviderCredentials,
    request: &LlmRequest,
    auth: AuthStyle,
    use_json_response_format: bool,
    azure_deployment: bool,
) -> Result<LlmResponse, AiRestError> {
    let started = std::time::Instant::now();
    let url = if azure_deployment {
        let api_version = creds
            .api_version
            .as_deref()
            .unwrap_or("2024-02-15-preview");
        format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            creds.base_url.trim_end_matches('/'),
            request.model,
            api_version
        )
    } else {
        format!(
            "{}/chat/completions",
            creds.base_url.trim_end_matches('/')
        )
    };

    let messages = build_openai_messages(request);
    let tools = request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| OpenAiTool {
                r#type: "function",
                function: OpenAiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect::<Vec<_>>()
    });

    let json_format = use_json_response_format
        && request.json_response
        && tools.is_none();

    let body = ChatCompletionRequest {
        model: if azure_deployment {
            None
        } else {
            Some(request.model.clone())
        },
        messages,
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        tools,
        response_format: json_format.then(|| ResponseFormat {
            r#type: "json_object".to_string(),
        }),
    };

    let mut req = client.post(url).json(&body);
    req = match auth {
        AuthStyle::Bearer if !creds.api_key.is_empty() => req.bearer_auth(&creds.api_key),
        AuthStyle::ApiKeyHeader if !creds.api_key.is_empty() => req.header("api-key", &creds.api_key),
        AuthStyle::Bearer | AuthStyle::ApiKeyHeader | AuthStyle::None => req,
    };

    let response = req
        .send()
        .await
        .map_err(|e| provider_error("request failed", e.to_string()))?;
    parse_openai_compatible_response(response, started).await
}

fn build_openai_messages(request: &LlmRequest) -> Vec<OpenAiChatMessage> {
    let source = request.messages.clone().unwrap_or_else(|| {
        InternalMessage::initial_conversation(&request.system_prompt, &request.user_prompt)
    });

    source
        .into_iter()
        .filter_map(|msg| match msg {
            InternalMessage::System { content } => Some(OpenAiChatMessage {
                role: "system".to_string(),
                content: Some(content),
                tool_calls: None,
                tool_call_id: None,
            }),
            InternalMessage::User { content } => Some(OpenAiChatMessage {
                role: "user".to_string(),
                content: Some(content),
                tool_calls: None,
                tool_call_id: None,
            }),
            InternalMessage::Assistant {
                content,
                tool_calls,
            } => {
                let openai_tool_calls = if tool_calls.is_empty() {
                    None
                } else {
                    Some(
                        tool_calls
                            .into_iter()
                            .map(|tc| OpenAiToolCall {
                                id: tc.id,
                                r#type: "function",
                                function: OpenAiCalledFunction {
                                    name: tc.name,
                                    arguments: serde_json::to_string(&tc.arguments)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                },
                            })
                            .collect(),
                    )
                };
                Some(OpenAiChatMessage {
                    role: "assistant".to_string(),
                    content,
                    tool_calls: openai_tool_calls,
                    tool_call_id: None,
                })
            }
            InternalMessage::Tool {
                tool_call_id,
                content,
            } => Some(OpenAiChatMessage {
                role: "tool".to_string(),
                content: Some(content),
                tool_calls: None,
                tool_call_id: Some(tool_call_id),
            }),
        })
        .collect()
}

async fn parse_openai_compatible_response(
    response: reqwest::Response,
    started: std::time::Instant,
) -> Result<LlmResponse, AiRestError> {
    let status = response.status();
    let response_body: Value = response.json().await.map_err(|e| {
        provider_error("failed to parse response JSON", e.to_string())
    })?;

    if !status.is_success() {
        return Err(provider_error_with_body(
            "OpenAI-compatible API returned an error",
            status.as_u16(),
            response_body,
        ));
    }

    let message = response_body
        .pointer("/choices/0/message")
        .ok_or_else(|| {
            provider_error_with_body(
                "OpenAI-compatible response missing message",
                status.as_u16(),
                response_body.clone(),
            )
        })?;

    let content = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let tool_calls = message
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?.to_string();
                    let func = tc.get("function")?;
                    let name = func.get("name")?.as_str()?.to_string();
                    let args_str = func.get("arguments")?.as_str().unwrap_or("{}");
                    let arguments: Value =
                        serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
                    Some(ToolCall { id, name, arguments })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if content.is_empty() && tool_calls.is_empty() {
        return Err(provider_error_with_body(
            "OpenAI-compatible response missing content and tool_calls",
            status.as_u16(),
            response_body.clone(),
        ));
    }

    let usage = response_body.get("usage").map(|u| TokenUsage {
        input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()),
        output_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()),
    });

    Ok(LlmResponse {
        content,
        tool_calls,
        usage,
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
struct ChatCompletionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    messages: Vec<OpenAiChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    r#type: &'static str,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall {
    id: String,
    r#type: &'static str,
    function: OpenAiCalledFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiCalledFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String,
}
