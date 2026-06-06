use crate::definitions::{LocalToolSpec, ToolsConfig};
use crate::errors::{AiRestError, ErrorType};
use crate::llm::{tool_api_name, ToolDefinition};
use crate::mcp::client::McpManager;
use crate::mcp::local_tool::LocalToolRuntime;

pub const LOCAL_TOOL_SERVER: &str = "local";

#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub qualified_name: String,
    pub api_name: String,
    pub server: String,
    pub tool: String,
    pub definition: ToolDefinition,
    pub source: ToolSource,
}

#[derive(Debug, Clone)]
pub enum ToolSource {
    Mcp,
    Local(LocalToolRuntime),
}

pub struct ToolRegistry {
    pub tools: Vec<RegisteredTool>,
    manager: Option<McpManager>,
}

impl ToolRegistry {
    pub async fn build(tools_cfg: &ToolsConfig) -> Result<Self, AiRestError> {
        let allow = tools_cfg.allow.as_deref().unwrap_or(&[]);
        let servers = tools_cfg.mcp_servers.as_deref().unwrap_or_default();
        let local = tools_cfg.local.as_deref().unwrap_or_default();

        if servers.is_empty() && local.is_empty() {
            return Err(AiRestError::new(
                ErrorType::McpTool,
                "tools must declare mcpServers and/or local tools.",
            ));
        }

        let mut registry_tools = Vec::new();

        for spec in local {
            register_local_tool(spec, allow, &mut registry_tools)?;
        }

        let manager = if servers.is_empty() {
            None
        } else {
            let manager = McpManager::from_endpoint(servers).await?;
            for server in servers {
                let Some(client) = manager.get(&server.name) else {
                    continue;
                };
                let listed = client.list_tools().await?;
                for info in listed {
                    let qualified = format!("{}/{}", server.name, info.name);
                    if !allow.is_empty() && !allow.iter().any(|a| a == &qualified) {
                        continue;
                    }
                    let api_name = tool_api_name(&server.name, &info.name);
                    registry_tools.push(RegisteredTool {
                        qualified_name: qualified.clone(),
                        api_name: api_name.clone(),
                        server: server.name.clone(),
                        tool: info.name.clone(),
                        definition: ToolDefinition {
                            name: api_name,
                            description: info.description,
                            parameters: info.input_schema,
                        },
                        source: ToolSource::Mcp,
                    });
                }
            }
            Some(manager)
        };

        if registry_tools.is_empty() {
            return Err(AiRestError::new(
                ErrorType::McpTool,
                "No tools available after applying tools.allow.",
            ));
        }

        Ok(Self {
            tools: registry_tools,
            manager,
        })
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|t| t.definition.clone())
            .collect()
    }

    pub fn resolve_api_name(&self, api_name: &str) -> Option<&RegisteredTool> {
        self.tools.iter().find(|t| t.api_name == api_name)
    }

    pub fn manager(&self) -> Option<&McpManager> {
        self.manager.as_ref()
    }
}

fn register_local_tool(
    spec: &LocalToolSpec,
    allow: &[String],
    tools: &mut Vec<RegisteredTool>,
) -> Result<(), AiRestError> {
    let qualified = format!("{LOCAL_TOOL_SERVER}/{}", spec.name);
    if !allow.is_empty() && !allow.iter().any(|a| a == &qualified) {
        return Ok(());
    }

    let runtime = LocalToolRuntime::from_spec(spec)?;
    let api_name = tool_api_name(LOCAL_TOOL_SERVER, &spec.name);
    tools.push(RegisteredTool {
        qualified_name: qualified,
        api_name: api_name.clone(),
        server: LOCAL_TOOL_SERVER.to_string(),
        tool: spec.name.clone(),
        definition: ToolDefinition {
            name: api_name,
            description: spec.llm_description(),
            parameters: spec.input_schema.clone(),
        },
        source: ToolSource::Local(runtime),
    });
    Ok(())
}
