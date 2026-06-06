use anyhow::{bail, Result};

use super::types::EndpointDefinition;

pub fn validate_endpoint_definition(def: &EndpointDefinition) -> Result<()> {
    if def.name.trim().is_empty() {
        bail!("'name' is required and cannot be empty");
    }

    if def.version.trim().is_empty() {
        bail!("'version' is required and cannot be empty");
    }

    if !def.is_get() && !def.is_post() {
        bail!(
            "Only GET and POST methods are supported, got '{}'",
            def.method
        );
    }

    if def.path.trim().is_empty() {
        bail!("'path' is required and cannot be empty");
    }

    if !def.path.starts_with('/') {
        bail!("'path' must start with '/', got '{}'", def.path);
    }

    if def.system_prompt.trim().is_empty() {
        bail!("'systemPrompt' is required and cannot be empty");
    }

    validate_json_schema_object(&def.input_schema, "inputSchema")?;
    validate_json_schema_object(&def.output_schema, "outputSchema")?;

    def.model.provider_kind()?;

    if def.model.model.trim().is_empty() {
        bail!("'model.model' is required and cannot be empty");
    }

    if let Some(errors) = &def.errors {
        validate_error_override(errors.input_validation.as_ref(), "errors.inputValidation")?;
        validate_error_override(errors.authentication.as_ref(), "errors.authentication")?;
        validate_error_override(errors.prompt_rendering.as_ref(), "errors.promptRendering")?;
        validate_error_override(errors.model_provider.as_ref(), "errors.modelProvider")?;
        validate_error_override(errors.model_json_parse.as_ref(), "errors.modelJsonParse")?;
        validate_error_override(
            errors.model_output_validation.as_ref(),
            "errors.modelOutputValidation",
        )?;
        validate_error_override(errors.internal_server.as_ref(), "errors.internalServer")?;
        validate_error_override(errors.guardrail.as_ref(), "errors.guardrail")?;
        validate_error_override(errors.hook_execution.as_ref(), "errors.hookExecution")?;
        validate_error_override(errors.cache.as_ref(), "errors.cache")?;
    }

    if let Some(health) = &def.health {
        if let Some(status) = health.status {
            if !(200..600).contains(&status) {
                bail!("'health.status' must be an HTTP status between 200 and 599");
            }
        }
        if let Some(message) = &health.message {
            if message.trim().is_empty() {
                bail!("'health.message' cannot be empty when provided");
            }
        }
    }

    if let Some(guardrails) = &def.guardrails {
        for (index, spec) in guardrails.iter().enumerate() {
            if spec.module.trim().is_empty() {
                bail!("'guardrails[{index}].module' cannot be empty");
            }
            if spec.is_deno() {
                if spec.script.as_ref().is_none_or(|s| s.trim().is_empty())
                    && spec.path.as_ref().is_none_or(|p| p.trim().is_empty())
                {
                    bail!(
                        "'guardrails[{index}]' with runtime deno requires 'script' or 'path'"
                    );
                }
                if let Some(timeout) = spec.timeout_ms {
                    if timeout == 0 || timeout > 30_000 {
                        bail!("'guardrails[{index}].timeoutMs' must be between 1 and 30000");
                    }
                }
            } else if let Some(runtime) = &spec.runtime {
                if runtime != "builtin" {
                    bail!(
                        "'guardrails[{index}].runtime' must be 'builtin' or 'deno', got '{runtime}'"
                    );
                }
            }
        }
    }

    if let Some(cache) = &def.cache {
        if cache.enabled {
            if let Some(threshold) = cache.similarity_threshold {
                if !(0.0..=1.0).contains(&threshold) {
                    bail!("'cache.similarityThreshold' must be between 0 and 1");
                }
            }
            if let Some(embedder) = &cache.embedder {
                if let Some(provider) = embedder.provider.as_deref() {
                    match provider.to_ascii_lowercase().as_str() {
                        "hash" | "openai" => {}
                        other => bail!("'cache.embedder.provider' must be 'hash' or 'openai', got '{other}'"),
                    }
                }
            }
            if let Some(store) = &cache.store {
                if let Some(store_type) = store.r#type.as_deref() {
                    match store_type.to_ascii_lowercase().as_str() {
                        "memory" | "redb" => {}
                        other => bail!("'cache.store.type' must be 'memory' or 'redb', got '{other}'"),
                    }
                }
            }
        }
    }

    if let Some(hooks) = &def.hooks {
        if let Some(spec) = &hooks.pre_request {
            validate_hook_spec(spec, "hooks.preRequest")?;
        }
        if let Some(spec) = &hooks.post_input {
            validate_hook_spec(spec, "hooks.postInput")?;
        }
        if let Some(spec) = &hooks.pre_llm {
            validate_hook_spec(spec, "hooks.preLlm")?;
        }
        if let Some(spec) = &hooks.post_output {
            validate_hook_spec(spec, "hooks.postOutput")?;
        }
    }

    if let Some(tools) = &def.tools {
        let has_mcp = tools
            .mcp_servers
            .as_ref()
            .is_some_and(|servers| !servers.is_empty());
        let has_local = tools.local.as_ref().is_some_and(|local| !local.is_empty());

        if has_mcp || has_local {
            let allow = tools.allow.as_deref().unwrap_or(&[]);
            if allow.is_empty() {
                bail!(
                    "'tools.allow' must list at least one qualified tool when 'tools.mcpServers' or 'tools.local' is configured"
                );
            }
            for (index, entry) in allow.iter().enumerate() {
                if !entry.contains('/') {
                    bail!(
                        "'tools.allow[{index}]' must use qualified form 'serverName/tool_name' or 'local/tool_name', got '{entry}'"
                    );
                }
            }
        }

        if let Some(local) = &tools.local {
            for (index, spec) in local.iter().enumerate() {
                validate_local_tool_spec(spec, index)?;
            }
        }

        if let Some(servers) = &tools.mcp_servers {
            for (index, server) in servers.iter().enumerate() {
                if server.name.trim().is_empty() {
                    bail!("'tools.mcpServers[{index}].name' cannot be empty");
                }
                if server.transport.trim().is_empty() {
                    bail!("'tools.mcpServers[{index}].transport' cannot be empty");
                }
                match server.transport.as_str() {
                    "stdio" if server.command.as_ref().is_none_or(|c| c.trim().is_empty()) => {
                        bail!("'tools.mcpServers[{index}]' stdio transport requires command");
                    }
                    "http" | "streamableHttp" | "sse"
                        if server.url.as_ref().is_none_or(|u| u.trim().is_empty()) =>
                    {
                        bail!(
                            "'tools.mcpServers[{index}]' {} transport requires url",
                            server.transport
                        );
                    }
                    "stdio" | "http" | "streamableHttp" | "sse" => {}
                    other => bail!(
                        "'tools.mcpServers[{index}].transport' must be stdio, http, streamableHttp, or sse, got '{other}'"
                    ),
                }
            }
        }
    }

    if let Some(auth) = &def.auth {
        match auth.auth_type() {
            "jwt"
                if auth.jwt.is_none()
                    && std::env::var("AIREST_JWT_JWKS_URL").ok().filter(|v| !v.is_empty()).is_none() =>
            {
                bail!("JWT auth requires 'auth.jwt' configuration or AIREST_JWT_JWKS_URL");
            }
            "oauth2Introspect" if auth.oauth2.is_none() => {
                bail!("OAuth2 introspection requires 'auth.oauth2' configuration");
            }
            "trustGateway" if auth.trust_gateway.is_none() => {
                bail!("trustGateway auth requires 'auth.trustGateway' configuration");
            }
            _ => {}
        }
    }

    Ok(())
}

fn validate_hook_spec(spec: &super::types::HookSpec, field: &str) -> Result<()> {
    if spec.runtime.trim().is_empty() {
        bail!("'{field}.runtime' cannot be empty");
    }
    if spec.script.trim().is_empty() {
        bail!("'{field}.script' cannot be empty");
    }
    if let Some(perms) = &spec.permissions {
        if perms.iter().any(|perm| perm == "network:*") {
            bail!(
                "'{field}.permissions' must not include 'network:*'; use explicit hosts such as network:api.example.com"
            );
        }
        crate::hooks::permissions::validate_permission_tokens(perms)
            .map_err(|reason| anyhow::anyhow!("'{field}.permissions' invalid: {reason}"))?;
    }

    if spec.runtime == "deno" && crate::hooks::permissions::script_uses_network_fetch(&spec.script) {
        let perms = spec.permissions.as_deref().unwrap_or(&[]);
        if perms.is_empty()
            || crate::hooks::permissions::NetworkAllowlist::parse(perms).is_empty()
        {
            bail!(
                "'{field}' script calls fetch() but declares no network: permissions; add network:host or remove fetch"
            );
        }
    }

    Ok(())
}

fn validate_local_tool_spec(spec: &super::types::LocalToolSpec, index: usize) -> Result<()> {
    let field = format!("tools.local[{index}]");
    if spec.name.trim().is_empty() {
        bail!("'{field}.name' cannot be empty");
    }
    if spec.description.trim().is_empty() {
        bail!("'{field}.description' cannot be empty");
    }
    validate_json_schema_object(&spec.input_schema, &format!("{field}.inputSchema"))?;
    if spec.runtime != "deno" {
        bail!("'{field}.runtime' must be 'deno'");
    }
    let script = spec.script.as_deref().unwrap_or("").trim();
    let path = spec.path.as_deref().unwrap_or("").trim();
    if script.is_empty() && path.is_empty() {
        bail!("'{field}' requires 'script' or 'path'");
    }
    if !script.is_empty() {
        if script.len() > 51_200 {
            bail!("'{field}.script' exceeds maximum allowed size");
        }
        for forbidden in ["Deno.read", "Deno.write", "require(", "process.", "Deno.run"] {
            if script.contains(forbidden) {
                bail!("'{field}.script' uses forbidden sandbox operation: {forbidden}");
            }
        }
        if let Some(perms) = &spec.permissions {
            if perms.iter().any(|perm| perm == "network:*") {
                bail!("'{field}.permissions' must not include 'network:*'");
            }
            crate::hooks::permissions::validate_permission_tokens(perms)
                .map_err(|reason| anyhow::anyhow!("'{field}.permissions' invalid: {reason}"))?;
        }
        if crate::hooks::permissions::script_uses_network_fetch(script) {
            let perms = spec.permissions.as_deref().unwrap_or(&[]);
            if perms.is_empty()
                || crate::hooks::permissions::NetworkAllowlist::parse(perms).is_empty()
            {
                bail!(
                    "'{field}' script calls fetch() but declares no network: permissions"
                );
            }
        }
    }
    Ok(())
}

fn validate_error_override(override_cfg: Option<&super::types::ErrorOverride>, field: &str) -> Result<()> {
    let Some(cfg) = override_cfg else {
        return Ok(());
    };

    if cfg.message.trim().is_empty() {
        bail!("'{field}.message' cannot be empty");
    }

    if let Some(status) = cfg.status {
        if !(400..600).contains(&status) {
            bail!("'{field}.status' must be an HTTP error status between 400 and 599");
        }
    }

    Ok(())
}

fn validate_json_schema_object(schema: &serde_json::Value, field: &str) -> Result<()> {
    let obj = schema
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("'{field}' must be a JSON Schema object"))?;

    if obj.get("type").and_then(|t| t.as_str()) != Some("object") {
        bail!("'{field}' must have type 'object'");
    }

    Ok(())
}
