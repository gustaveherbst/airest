//! Tracing span helpers aligned with ROADMAP OTEL structure.

use crate::auth::AuthContext;
use tracing::Span;

pub fn apply_auth_attrs(span: &Span, auth: Option<&AuthContext>) {
    if let Some(auth) = auth {
        if let Some(subject) = &auth.subject {
            span.record("airest.auth.subject", subject.as_str());
        }
        if let Some(tenant) = &auth.tenant_id {
            span.record("airest.auth.tenant", tenant.as_str());
        }
    }
}

pub fn request(
    endpoint: &str,
    version: &str,
    request_id: &str,
    method: &str,
    path: &str,
    auth: Option<&AuthContext>,
) -> Span {
    let span = tracing::info_span!(
        "airest.request",
        airest.endpoint = %endpoint,
        airest.endpoint.version = %version,
        airest.request_id = %request_id,
        airest.http.method = %method,
        airest.http.path = %path,
        airest.auth.subject = tracing::field::Empty,
        airest.auth.tenant = tracing::field::Empty,
    );
    apply_auth_attrs(&span, auth);
    span
}

pub fn validate_input(endpoint: &str, request_id: &str) -> Span {
    tracing::info_span!(
        "airest.validate_input",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
    )
}

pub fn render_prompt(endpoint: &str, request_id: &str) -> Span {
    tracing::info_span!(
        "airest.render_prompt",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
    )
}

pub fn guardrail_module(
    endpoint: &str,
    request_id: &str,
    module: &str,
    runtime: &str,
    hook: &str,
) -> Span {
    tracing::info_span!(
        "airest.guardrails",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
        airest.guardrail.module = %module,
        airest.guardrail.runtime = %runtime,
        airest.guardrail.hook = %hook,
    )
}

pub fn llm_complete(
    endpoint: &str,
    request_id: &str,
    provider: &str,
    model: &str,
    attempt: u32,
) -> Span {
    tracing::info_span!(
        "airest.llm.complete",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
        gen_ai.system = %provider,
        gen_ai.request.model = %model,
        airest.llm.attempt = attempt,
    )
}

pub fn llm_retry(endpoint: &str, request_id: &str, reason: &str, attempt: u32) -> Span {
    tracing::info_span!(
        "airest.llm.retry",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
        airest.retry.reason = %reason,
        airest.llm.attempt = attempt,
    )
}

pub fn parse_json(endpoint: &str, request_id: &str) -> Span {
    tracing::info_span!(
        "airest.parse_json",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
    )
}

pub fn validate_output(endpoint: &str, request_id: &str, status: &str) -> Span {
    tracing::info_span!(
        "airest.validate_output",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
        airest.validation.status = %status,
    )
}

pub fn hook_execute(
    endpoint: &str,
    request_id: &str,
    hook: &str,
    runtime: &str,
) -> Span {
    tracing::info_span!(
        "airest.hook",
        endpoint = %endpoint,
        request_id = %request_id,
        hook_name = %hook,
        hook_runtime = %runtime,
    )
}

pub fn mcp_tool_call(
    endpoint: &str,
    request_id: &str,
    mcp_server: &str,
    tool_name: &str,
) -> Span {
    tracing::info_span!(
        "airest.mcp.tool",
        endpoint = %endpoint,
        request_id = %request_id,
        mcp_server = %mcp_server,
        tool_name = %tool_name,
    )
}

pub fn cache_lookup(endpoint: &str, request_id: &str) -> Span {
    tracing::info_span!(
        "airest.cache.lookup",
        airest.endpoint = %endpoint,
        airest.request_id = %request_id,
    )
}
