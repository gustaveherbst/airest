use serde_json::Value;

use crate::definitions::McpServerConfig;
use crate::errors::AiRestError;
use crate::mcp::transport::McpTransport;

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone)]
pub struct McpClient {
    transport: std::sync::Arc<McpTransport>,
}

impl McpClient {
    pub async fn connect(config: &McpServerConfig) -> Result<Self, AiRestError> {
        let http = reqwest::Client::new();
        let transport = std::sync::Arc::new(McpTransport::connect(config, &http).await?);
        let client = Self { transport };
        client.transport.initialize().await?;
        Ok(client)
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolInfo>, AiRestError> {
        let result = self.transport.call("tools/list", None).await?;
        let tools = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(tools
            .into_iter()
            .filter_map(|t| {
                let name = t.get("name")?.as_str()?.to_string();
                let description = t
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("MCP tool")
                    .to_string();
                let input_schema = t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object"}));
                Some(McpToolInfo {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect())
    }

    pub async fn invoke_tool(&self, name: &str, arguments: Value) -> Result<Value, AiRestError> {
        self.transport
            .call(
                "tools/call",
                Some(serde_json::json!({
                    "name": name,
                    "arguments": arguments
                })),
            )
            .await
    }
}

#[derive(Clone)]
pub struct McpManager {
    clients: std::collections::HashMap<String, McpClient>,
}

impl McpManager {
    pub async fn from_endpoint(servers: &[McpServerConfig]) -> Result<Self, AiRestError> {
        let mut clients = std::collections::HashMap::new();
        for server in servers {
            let client = McpClient::connect(server).await?;
            clients.insert(server.name.clone(), client);
        }
        Ok(Self { clients })
    }

    pub fn get(&self, name: &str) -> Option<&McpClient> {
        self.clients.get(name)
    }
}
