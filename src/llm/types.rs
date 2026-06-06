use std::str::FromStr;

use anyhow::{bail, Result};

use crate::llm::tools::{ChatMessage, TokenUsage, ToolCall, ToolDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Openai,
    AzureOpenai,
    Anthropic,
    Gemini,
    Grok,
    Ollama,
}

impl ProviderKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::Openai),
            "azure_openai" | "azure-openai" | "azure" => Ok(Self::AzureOpenai),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" | "google" => Ok(Self::Gemini),
            "grok" | "xai" => Ok(Self::Grok),
            "ollama" => Ok(Self::Ollama),
            other => bail!(
                "Unsupported model provider '{other}'. Supported: openai, azure_openai, anthropic, gemini, grok, ollama"
            ),
        }
    }

    pub fn supports_native_tools(self) -> bool {
        matches!(self, Self::Openai | Self::AzureOpenai | Self::Anthropic | Self::Grok)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::AzureOpenai => "azure_openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Ollama => "ollama",
        }
    }
}

impl FromStr for ProviderKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub provider: ProviderKind,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    /// Multi-turn history; when set, replaces the default system+user pair.
    pub messages: Option<Vec<ChatMessage>>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<ToolDefinition>>,
    /// When true, request JSON object response format (disabled during tool rounds).
    pub json_response: bool,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub latency_ms: u64,
}

impl LlmResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}
