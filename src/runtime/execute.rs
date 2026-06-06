use serde_json::Value;
use tracing::{error, info, warn, Instrument};
use crate::otel::{
    cache_lookup, llm_complete, llm_retry, parse_json as parse_json_span,
    render_prompt as render_prompt_span, validate_input as validate_input_span,
    validate_output as validate_output_span, TelemetryState,
};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::cache::{CacheLookup, CacheStore};
use crate::config::Config;
use crate::definitions::EndpointDefinition;
use crate::errors::{AiRestError, ErrorResponse, ErrorType};
use crate::guardrails::{run_hook, GuardrailChain, GuardrailContext, GuardrailHook};
use crate::hooks::{
    run_post_input_hook, run_post_output_hook, run_pre_llm_hook, run_pre_request_hook,
};
use crate::llm::{LlmRequest, LlmRouter};
use crate::mcp::run_tool_loop;
use crate::prompts::{render_correction_prompt, RenderedPrompt};
use crate::runtime::parse_json::parse_model_json;
use crate::runtime::response::SuccessResponse;
use crate::validation::{validate_input, validate_output, ValidationError};

pub struct ExecutionResult {
    pub success: SuccessResponse,
}

pub struct ExecutionContext<'a> {
    pub config: &'a Config,
    pub llm: &'a LlmRouter,
    pub endpoint: &'a EndpointDefinition,
    pub request_id: String,
    pub input: Value,
    pub request_body_bytes: usize,
    pub auth: Option<AuthContext>,
    pub cache: Option<&'a CacheStore>,
    pub guardrail_chain: Option<&'a GuardrailChain>,
    pub telemetry: Option<&'a TelemetryState>,
}

pub async fn execute_request(ctx: ExecutionContext<'_>) -> Result<ExecutionResult, ErrorResponse> {
    let started = std::time::Instant::now();
    let endpoint = ctx.endpoint;
    let policies = endpoint.policies();
    let request_id = ctx.request_id.clone();
    let mut input = ctx.input;
    let mut cache_miss_meta: Option<crate::cache::CacheMeta> = None;

    input = run_pre_request_hook(endpoint, &request_id, input)
        .await
        .map_err(|err| error_response(&request_id, endpoint, err))?;

    input = run_guardrail_input(
        ctx.guardrail_chain,
        endpoint,
        &request_id,
        GuardrailHook::PreInput,
        input,
        ctx.request_body_bytes,
        ctx.auth.as_ref(),
        None,
        None,
        None,
        ctx.telemetry,
    )
    .await?;

    if policies.validate_input {
        let validation_span = validate_input_span(&endpoint.name, &request_id);
        let validation_result = async { validate_input(&endpoint.input_schema, &input) }
            .instrument(validation_span)
            .await;
        if let Err(err) = validation_result {
            if let Some(telemetry) = ctx.telemetry {
                if telemetry.endpoint_enabled(endpoint) {
                    telemetry.record_validation_failure(&endpoint.name, "input");
                }
            }
            return Err(error_response(
                &request_id,
                endpoint,
                AiRestError::with_details(
                    ErrorType::InputValidation,
                    if endpoint.is_get() {
                        "Request query parameters do not match input schema."
                    } else {
                        "Request body does not match input schema."
                    },
                    validation_details(err),
                ),
            ));
        }
    }

    input = run_guardrail_input(
        ctx.guardrail_chain,
        endpoint,
        &request_id,
        GuardrailHook::PostInput,
        input,
        ctx.request_body_bytes,
        ctx.auth.as_ref(),
        None,
        None,
        None,
        ctx.telemetry,
    )
    .await?;

    input = run_post_input_hook(endpoint, &request_id, input)
        .await
        .map_err(|err| error_response(&request_id, endpoint, err))?;

    if let (Some(store), Some(cache_cfg)) = (ctx.cache, &endpoint.cache) {
        if cache_cfg.enabled {
            let cache_span = cache_lookup(&endpoint.name, &request_id);
            match async { store.lookup(endpoint, cache_cfg, &input, ctx.auth.as_ref()).await }
                .instrument(cache_span)
                .await
            {
                Ok(CacheLookup::Hit { output, cache_meta }) => {
                    if let Some(telemetry) = ctx.telemetry {
                        if telemetry.endpoint_enabled(endpoint) {
                            telemetry.record_cache(
                                &endpoint.name,
                                true,
                                cache_meta.similarity,
                                cache_meta.estimated_tokens_saved,
                            );
                        }
                    }
                    let total_latency_ms = started.elapsed().as_millis() as u64;
                    return Ok(ExecutionResult {
                        success: SuccessResponse::new_with_cache(
                            output,
                            request_id,
                            endpoint.name.clone(),
                            endpoint.version.clone(),
                            endpoint.model.model.clone(),
                            total_latency_ms,
                            Some(cache_meta),
                        ),
                    });
                }
                Ok(CacheLookup::Miss) => {
                    if let Some(telemetry) = ctx.telemetry {
                        if telemetry.endpoint_enabled(endpoint) {
                            telemetry.record_cache(&endpoint.name, false, None, None);
                        }
                    }
                    cache_miss_meta = Some(crate::cache::CacheMeta {
                        hit: false,
                        similarity: None,
                        cached_request_id: None,
                        latency_ms: started.elapsed().as_millis() as u64,
                        estimated_tokens_saved: None,
                    });
                }
                Err(err) => {
                    warn!(request_id = %request_id, error = %err, "Cache lookup failed, continuing");
                }
            }
        }
    }

    input = run_pre_llm_hook(endpoint, &request_id, input)
        .await
        .map_err(|err| error_response(&request_id, endpoint, err))?;

    let render_span = render_prompt_span(&endpoint.name, &request_id);
    let rendered = match async {
        crate::prompts::render_prompt(
            &endpoint.system_prompt,
            endpoint.user_prompt_template.as_deref(),
            &endpoint.output_schema,
            &input,
        )
    }
    .instrument(render_span)
    .await
    {
        Ok(prompt) => prompt,
        Err(err) => return Err(error_response(&request_id, endpoint, err)),
    };

    run_guardrail_input(
        ctx.guardrail_chain,
        endpoint,
        &request_id,
        GuardrailHook::PreLlm,
        input.clone(),
        ctx.request_body_bytes,
        ctx.auth.as_ref(),
        Some(&rendered.system),
        Some(&rendered.user),
        None,
        ctx.telemetry,
    )
    .await?;

    if policies.log_requests {
        let logged_input = if policies.redact_inputs {
            Value::String("[REDACTED]".to_string())
        } else {
            input.clone()
        };
        info!(
            request_id = %request_id,
            endpoint = %endpoint.name,
            input = ?logged_input,
            "Processing aiREST request"
        );
    }

    let tool_timeout_ms = endpoint
        .tools
        .as_ref()
        .and_then(|t| t.tool_timeout_ms)
        .unwrap_or(policies.tool_timeout_ms);

    let llm_result = if endpoint.tools.is_some() {
        run_tool_loop(
            ctx.llm,
            endpoint,
            &request_id,
            &rendered,
            policies.max_tool_rounds,
            tool_timeout_ms,
            ctx.telemetry,
        )
        .await
    } else {
        call_with_retries(
            ctx.llm,
            endpoint,
            &request_id,
            &rendered,
            policies.max_retries,
            policies.retry_on_invalid_json,
            policies.retry_on_invalid_schema,
            policies.strip_markdown_code_fences,
            policies.validate_output,
            ctx.telemetry,
        )
        .await
        .map(|(output, latency, raw)| (output, latency, raw))
    };

    let (mut output, llm_latency_ms, llm_raw) = match llm_result {
        Ok(result) => result,
        Err(err) => return Err(error_response(&request_id, endpoint, err)),
    };

    if let Some(raw) = llm_raw.as_deref() {
        run_guardrail_input(
            ctx.guardrail_chain,
            endpoint,
            &request_id,
            GuardrailHook::PostLlm,
            input.clone(),
            ctx.request_body_bytes,
            ctx.auth.as_ref(),
            Some(&rendered.system),
            Some(&rendered.user),
            Some(raw),
            ctx.telemetry,
        )
        .await?;
    }

    output = run_post_output_hook(endpoint, &request_id, output)
        .await
        .map_err(|err| error_response(&request_id, endpoint, err))?;

    output = run_guardrail_output(
        ctx.guardrail_chain,
        endpoint,
        &request_id,
        input.clone(),
        output,
        ctx.request_body_bytes,
        ctx.auth.as_ref(),
        ctx.telemetry,
    )
    .await?;

    if policies.log_responses {
        let logged_output = if policies.redact_outputs {
            Value::String("[REDACTED]".to_string())
        } else {
            output.clone()
        };
        info!(
            request_id = %request_id,
            endpoint = %endpoint.name,
            output = ?logged_output,
            "aiREST response generated"
        );
    }

    if let (Some(store), Some(cache_cfg)) = (ctx.cache, &endpoint.cache) {
        if cache_cfg.enabled {
            let store_allowed = run_guardrail_cache_write(
                ctx.guardrail_chain,
                endpoint,
                &request_id,
                input.clone(),
                output.clone(),
                ctx.request_body_bytes,
                ctx.auth.as_ref(),
                cache_cfg,
                ctx.telemetry,
            )
            .await?;
            if store_allowed {
                let _ = store
                    .store(endpoint, cache_cfg, &input, &output, ctx.auth.as_ref())
                    .await;
            }
        }
    }

    let total_latency_ms = started.elapsed().as_millis() as u64;
    if let Some(meta) = cache_miss_meta.as_mut() {
        meta.latency_ms = total_latency_ms;
    }

    info!(
        request_id = %request_id,
        endpoint = %endpoint.name,
        model = %endpoint.model.model,
        latency_ms = total_latency_ms,
        validation_status = "valid",
        "aiREST request completed"
    );

    Ok(ExecutionResult {
        success: SuccessResponse::new_with_cache(
            output,
            request_id,
            endpoint.name.clone(),
            endpoint.version.clone(),
            endpoint.model.model.clone(),
            llm_latency_ms,
            cache_miss_meta,
        ),
    })
}

async fn run_guardrail_input(
    chain: Option<&GuardrailChain>,
    endpoint: &EndpointDefinition,
    request_id: &str,
    hook: GuardrailHook,
    input: Value,
    request_body_bytes: usize,
    auth: Option<&AuthContext>,
    rendered_system: Option<&str>,
    rendered_user: Option<&str>,
    llm_raw: Option<&str>,
    telemetry: Option<&TelemetryState>,
) -> Result<Value, ErrorResponse> {
    let ctx = GuardrailContext {
        request_id,
        endpoint,
        hook,
        input,
        rendered_system,
        rendered_user,
        llm_raw,
        output: None,
        request_body_bytes,
    };
    match run_hook(chain, endpoint, hook, ctx, auth, telemetry).await {
        Ok(ctx) => Ok(ctx.input),
        Err(err) => Err(error_response(request_id, endpoint, err)),
    }
}

async fn run_guardrail_cache_write(
    chain: Option<&GuardrailChain>,
    endpoint: &EndpointDefinition,
    request_id: &str,
    input: Value,
    output: Value,
    request_body_bytes: usize,
    auth: Option<&AuthContext>,
    cache_cfg: &crate::definitions::CacheConfig,
    telemetry: Option<&TelemetryState>,
) -> Result<bool, ErrorResponse> {
    let ctx = GuardrailContext {
        request_id,
        endpoint,
        hook: GuardrailHook::PreCacheWrite,
        input,
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: Some(output),
        request_body_bytes,
    };
    match run_hook(chain, endpoint, GuardrailHook::PreCacheWrite, ctx, auth, telemetry).await {
        Ok(_) => Ok(true),
        Err(err) if cache_cfg.bypass_on_guardrail_block.unwrap_or(true)
            && err.error_type() == ErrorType::GuardrailViolation =>
        {
            warn!(
                request_id = %request_id,
                endpoint = %endpoint.name,
                "Cache write blocked by guardrail; bypassOnGuardrailBlock enabled"
            );
            Ok(false)
        }
        Err(err) => Err(error_response(request_id, endpoint, err)),
    }
}

async fn run_guardrail_output(
    chain: Option<&GuardrailChain>,
    endpoint: &EndpointDefinition,
    request_id: &str,
    input: Value,
    output: Value,
    request_body_bytes: usize,
    auth: Option<&AuthContext>,
    telemetry: Option<&TelemetryState>,
) -> Result<Value, ErrorResponse> {
    let ctx = GuardrailContext {
        request_id,
        endpoint,
        hook: GuardrailHook::PostOutput,
        input,
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: Some(output.clone()),
        request_body_bytes,
    };
    match run_hook(chain, endpoint, GuardrailHook::PostOutput, ctx, auth, telemetry).await {
        Ok(ctx) => Ok(ctx.output.unwrap_or(output)),
        Err(err) => Err(error_response(request_id, endpoint, err)),
    }
}

async fn call_with_retries(
    llm: &LlmRouter,
    endpoint: &EndpointDefinition,
    request_id: &str,
    initial_prompt: &RenderedPrompt,
    max_retries: u32,
    retry_on_invalid_json: bool,
    retry_on_invalid_schema: bool,
    strip_fences: bool,
    validate_output_flag: bool,
    telemetry: Option<&TelemetryState>,
) -> Result<(Value, u64, Option<String>), AiRestError> {
    let mut attempt = 0u32;
    let mut current_prompt = initial_prompt.clone();
    let mut last_errors: Vec<String>;
    let mut total_latency_ms = 0u64;
    let mut last_raw;

    loop {
        let provider = endpoint.model.provider_kind().map_err(|err| {
            AiRestError::with_details(
                ErrorType::ModelProvider,
                err.to_string(),
                serde_json::json!({ "provider": endpoint.model.provider }),
            )
        })?;

        let llm_request = LlmRequest {
            provider,
            model: endpoint.model.model.clone(),
            system_prompt: current_prompt.system.clone(),
            user_prompt: current_prompt.user.clone(),
            messages: None,
            temperature: endpoint.model.temperature,
            max_tokens: endpoint.model.max_tokens,
            tools: None,
            json_response: true,
        };

        let provider_name = endpoint.model.provider.clone();
        let llm_span = llm_complete(
            &endpoint.name,
            request_id,
            &provider_name,
            &endpoint.model.model,
            attempt + 1,
        );
        let response = async { llm.complete(llm_request).await }
            .instrument(llm_span)
            .await?;
        total_latency_ms += response.latency_ms;
        last_raw = Some(response.content.clone());

        if let Some(telemetry) = telemetry {
            if telemetry.endpoint_enabled(endpoint) {
                telemetry.record_llm(
                    &endpoint.name,
                    &provider_name,
                    &endpoint.model.model,
                    response.latency_ms,
                );
                if let Some(usage) = &response.usage {
                    telemetry.record_token_usage(
                        &endpoint.name,
                        &provider_name,
                        &endpoint.model.model,
                        usage,
                    );
                }
            }
        }

        let parse_span = parse_json_span(&endpoint.name, request_id);
        let parsed_result = async { parse_model_json(&response.content, strip_fences) }
            .instrument(parse_span)
            .await;

        match parsed_result {
            Ok(parsed) => {
                if validate_output_flag {
                    let validation_span =
                        validate_output_span(&endpoint.name, request_id, "validating");
                    let validation_result = async {
                        validate_output(&endpoint.output_schema, &parsed)
                    }
                    .instrument(validation_span)
                    .await;

                    if let Err(err) = validation_result {
                        last_errors = validation_error_messages(err);
                        if let Some(telemetry) = telemetry {
                            if telemetry.endpoint_enabled(endpoint) {
                                telemetry.record_validation_failure(&endpoint.name, "output");
                            }
                        }
                        if retry_on_invalid_schema && attempt < max_retries {
                            attempt += 1;
                            if let Some(telemetry) = telemetry {
                                if telemetry.endpoint_enabled(endpoint) {
                                    telemetry.record_llm_retry(&endpoint.name, "invalid_schema");
                                }
                            }
                            let retry_span = llm_retry(
                                &endpoint.name,
                                request_id,
                                "invalid_schema",
                                attempt,
                            );
                            let _retry_guard = retry_span.enter();
                            warn!(
                                endpoint = %endpoint.name,
                                attempt,
                                "Retrying due to output schema validation failure"
                            );
                            current_prompt = RenderedPrompt {
                                system: initial_prompt.system.clone(),
                                user: render_correction_prompt(
                                    &endpoint.output_schema,
                                    &last_errors,
                                ),
                            };
                            continue;
                        }

                        return Err(AiRestError::with_details(
                            ErrorType::ModelOutputValidation,
                            if attempt > 0 {
                                "The model response did not match the required output schema after retry attempts."
                            } else {
                                "The model response did not match the output schema."
                            },
                            serde_json::json!({ "errors": last_errors }),
                        ));
                    }
                }

                return Ok((parsed, total_latency_ms, last_raw));
            }
            Err(parse_err) => {
                last_errors = vec![parse_err.clone()];
                if let Some(telemetry) = telemetry {
                    if telemetry.endpoint_enabled(endpoint) {
                        telemetry.record_validation_failure(&endpoint.name, "parse_json");
                    }
                }
                if retry_on_invalid_json && attempt < max_retries {
                    attempt += 1;
                    if let Some(telemetry) = telemetry {
                        if telemetry.endpoint_enabled(endpoint) {
                            telemetry.record_llm_retry(&endpoint.name, "invalid_json");
                        }
                    }
                    let retry_span =
                        llm_retry(&endpoint.name, request_id, "invalid_json", attempt);
                    let _retry_guard = retry_span.enter();
                    warn!(
                        endpoint = %endpoint.name,
                        attempt,
                        "Retrying due to JSON parse failure"
                    );
                    current_prompt = RenderedPrompt {
                        system: initial_prompt.system.clone(),
                        user: render_correction_prompt(&endpoint.output_schema, &last_errors),
                    };
                    continue;
                }

                return Err(AiRestError::with_details(
                    ErrorType::ModelJsonParse,
                    "Failed to parse model response as JSON",
                    serde_json::json!({ "errors": last_errors }),
                ));
            }
        }
    }
}

fn validation_details(err: ValidationError) -> Value {
    match err {
        ValidationError::SchemaCompilation(msg) => {
            serde_json::json!({ "errors": [msg] })
        }
        ValidationError::ValidationFailed { errors } => {
            serde_json::json!({ "errors": errors })
        }
    }
}

fn validation_error_messages(err: ValidationError) -> Vec<String> {
    match err {
        ValidationError::SchemaCompilation(msg) => vec![msg],
        ValidationError::ValidationFailed { errors } => errors,
    }
}

fn error_response(
    request_id: &str,
    endpoint: &EndpointDefinition,
    error: AiRestError,
) -> ErrorResponse {
    error!(
        request_id = %request_id,
        endpoint = %endpoint.name,
        error_type = %error.error_type().as_str(),
        message = %error.message(),
        "aiREST request failed"
    );

    ErrorResponse::for_endpoint(request_id.to_string(), endpoint, error)
}

pub fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().simple())
}
