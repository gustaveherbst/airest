use std::time::Duration;

use serde_json::Value;
use tokio::time::timeout;
use tracing::Instrument;

use crate::definitions::EndpointDefinition;
use crate::errors::{AiRestError, ErrorType};
use crate::llm::{ChatMessage, LlmRequest, LlmRouter, ToolCall};
use crate::mcp::local_tool::execute_local_tool;
use crate::mcp::registry::{ToolRegistry, ToolSource};
use crate::mcp::tool_prompt::augment_prompt_for_tools;
use crate::otel::{mcp_tool_call, TelemetryState};
use crate::prompts::RenderedPrompt;
use crate::runtime::parse_json::parse_model_json;
use crate::validation::validate_output;

pub async fn run_tool_loop(
    llm: &LlmRouter,
    endpoint: &EndpointDefinition,
    request_id: &str,
    prompt: &RenderedPrompt,
    max_rounds: u32,
    tool_timeout_ms: u64,
    telemetry: Option<&TelemetryState>,
) -> Result<(Value, u64, Option<String>), AiRestError> {
    let tools_cfg = endpoint.tools.as_ref().ok_or_else(|| {
        AiRestError::new(ErrorType::McpTool, "Endpoint has no tools configuration.")
    })?;

    let registry = ToolRegistry::build(tools_cfg).await?;

    let provider = endpoint.model.provider_kind().map_err(|err| {
        AiRestError::with_details(
            ErrorType::ModelProvider,
            err.to_string(),
            serde_json::json!({ "provider": endpoint.model.provider }),
        )
    })?;

    let native_tools = provider.supports_native_tools();
    let tool_definitions = if native_tools {
        Some(registry.definitions())
    } else {
        None
    };

    let effective_prompt = augment_prompt_for_tools(prompt, &registry.tools, native_tools);

    let mut messages =
        ChatMessage::initial_conversation(&effective_prompt.system, &effective_prompt.user);
    let mut total_latency_ms = 0u64;
    let mut last_raw;

    for round in 0..=max_rounds {
        let llm_request = LlmRequest {
            provider,
            model: endpoint.model.model.clone(),
            system_prompt: effective_prompt.system.clone(),
            user_prompt: effective_prompt.user.clone(),
            messages: Some(messages.clone()),
            temperature: endpoint.model.temperature,
            max_tokens: endpoint.model.max_tokens,
            tools: tool_definitions.clone(),
            json_response: tool_definitions.is_none(),
        };

        let response = llm.complete(llm_request).await?;
        total_latency_ms += response.latency_ms;
        last_raw = Some(
            if response.content.is_empty() {
                serde_json::to_string(&response.tool_calls).unwrap_or_default()
            } else {
                response.content.clone()
            },
        );

        if let Some(usage) = &response.usage {
            if let Some(telemetry) = telemetry {
                if telemetry.endpoint_enabled(endpoint) {
                    telemetry.record_token_usage(
                        &endpoint.name,
                        &endpoint.model.provider,
                        &endpoint.model.model,
                        usage,
                    );
                }
            }
        }

        if !response.content.is_empty() {
            if let Ok(parsed) = parse_model_json(&response.content, true) {
                if validate_output(&endpoint.output_schema, &parsed).is_ok() {
                    return Ok((parsed, total_latency_ms, last_raw));
                }
            }
        }

        if response.tool_calls.is_empty() {
            if round >= max_rounds {
                break;
            }
            if !native_tools {
                try_prompt_tool_fallback(
                    &response.content,
                    &registry,
                    tool_timeout_ms,
                    endpoint,
                    request_id,
                    telemetry,
                    &mut messages,
                )
                .await?;
            }
            continue;
        }

        messages.push(ChatMessage::Assistant {
            content: if response.content.is_empty() {
                None
            } else {
                Some(response.content.clone())
            },
            tool_calls: response.tool_calls.clone(),
        });

        for call in &response.tool_calls {
            let result = execute_tool_call(
                &registry,
                call,
                tool_timeout_ms,
                endpoint,
                request_id,
                telemetry,
            )
            .await?;
            append_tool_result(provider, &mut messages, call, &result);
        }

        if round >= max_rounds {
            break;
        }
    }

    Err(AiRestError::new(
        ErrorType::McpTool,
        "Tool loop exceeded maxToolRounds without valid output.",
    ))
}

async fn execute_tool_call(
    registry: &ToolRegistry,
    call: &ToolCall,
    tool_timeout_ms: u64,
    endpoint: &EndpointDefinition,
    request_id: &str,
    telemetry: Option<&TelemetryState>,
) -> Result<String, AiRestError> {
    let registered = registry.resolve_api_name(&call.name).ok_or_else(|| {
        AiRestError::with_details(
            ErrorType::McpTool,
            "Unknown tool name from model.",
            serde_json::json!({ "tool": call.name }),
        )
    })?;

    let span = mcp_tool_call(
        &endpoint.name,
        request_id,
        &registered.server,
        &registered.tool,
    );

    let result = timeout(
        Duration::from_millis(tool_timeout_ms),
        async {
            match &registered.source {
                ToolSource::Local(runtime) => {
                    let value = execute_local_tool(
                        runtime,
                        &registered.tool,
                        request_id,
                        call.arguments.clone(),
                    )
                    .await?;
                    Ok::<String, AiRestError>(
                        serde_json::to_string(&value).unwrap_or_else(|_| value.to_string()),
                    )
                }
                ToolSource::Mcp => {
                    let manager = registry.manager().ok_or_else(|| {
                        AiRestError::new(ErrorType::McpTool, "MCP manager not initialized.")
                    })?;
                    let client = manager.get(&registered.server).ok_or_else(|| {
                        AiRestError::with_details(
                            ErrorType::McpTool,
                            "MCP server not connected.",
                            serde_json::json!({ "server": registered.server }),
                        )
                    })?;

                    let value = client
                        .invoke_tool(&registered.tool, call.arguments.clone())
                        .await?;
                    Ok(serde_json::to_string(&value).unwrap_or_else(|_| value.to_string()))
                }
            }
        }
        .instrument(span),
    )
    .await
    .map_err(|_| {
        AiRestError::with_details(
            ErrorType::McpTool,
            "Tool call timed out.",
            serde_json::json!({
                "tool": registered.qualified_name,
                "timeoutMs": tool_timeout_ms,
            }),
        )
    })??;

    if let Some(telemetry) = telemetry {
        if telemetry.endpoint_enabled(endpoint) {
            telemetry.record_mcp_tool(&endpoint.name, &registered.qualified_name, true);
        }
    }

    Ok(result)
}

fn append_tool_result(
    provider: crate::llm::ProviderKind,
    messages: &mut Vec<ChatMessage>,
    call: &ToolCall,
    result: &str,
) {
    let _ = provider;
    messages.push(ChatMessage::Tool {
        tool_call_id: call.id.clone(),
        content: result.to_string(),
    });
}

async fn try_prompt_tool_fallback(
    content: &str,
    registry: &ToolRegistry,
    tool_timeout_ms: u64,
    endpoint: &EndpointDefinition,
    request_id: &str,
    telemetry: Option<&TelemetryState>,
    messages: &mut Vec<ChatMessage>,
) -> Result<(), AiRestError> {
    let Some((server_name, tool_name, args)) = parse_prompt_tool_call(content) else {
        return Ok(());
    };

    let qualified = format!("{server_name}/{tool_name}");
    let call = ToolCall {
        id: format!("prompt_{qualified}"),
        name: registry
            .tools
            .iter()
            .find(|t| t.qualified_name == qualified)
            .map(|t| t.api_name.clone())
            .unwrap_or_else(|| crate::llm::tool_api_name(&server_name, &tool_name)),
        arguments: args,
    };

    let result = execute_tool_call(
        registry,
        &call,
        tool_timeout_ms,
        endpoint,
        request_id,
        telemetry,
    )
    .await?;

    messages.push(ChatMessage::User {
        content: format!("Tool result from {qualified}:\n{result}"),
    });
    Ok(())
}

fn parse_prompt_tool_call(content: &str) -> Option<(String, String, Value)> {
    let parsed: Value = serde_json::from_str(content).ok()?;
    let server = parsed.get("mcpServer")?.as_str()?.to_string();
    let tool = parsed.get("tool")?.as_str()?.to_string();
    let args = parsed
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    Some((server, tool, args))
}
