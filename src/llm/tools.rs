use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider-safe tool name (`server__tool_name`).
pub fn tool_api_name(server: &str, tool: &str) -> String {
    let server = server.replace('-', "_");
    let tool = tool.replace('-', "_");
    format!("{server}__{tool}")
}

pub fn parse_tool_api_name(api_name: &str) -> Option<(String, String)> {
    let (server, tool) = api_name.split_once("__")?;
    Some((server.replace('_', "-"), tool.replace('_', "-")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl ChatMessage {
    pub fn initial_conversation(system: &str, user: &str) -> Vec<Self> {
        vec![
            ChatMessage::System {
                content: system.to_string(),
            },
            ChatMessage::User {
                content: user.to_string(),
            },
        ]
    }
}
