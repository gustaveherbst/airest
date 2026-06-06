use anyhow::{bail, Context, Result};

use super::types::ProviderKind;

#[derive(Debug, Clone)]
pub struct ProviderCredentials {
    pub api_key: String,
    pub base_url: String,
    pub api_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub openai: ProviderCredentials,
    pub azure_openai: ProviderCredentials,
    pub anthropic: ProviderCredentials,
    pub gemini: ProviderCredentials,
    pub grok: ProviderCredentials,
    pub ollama: ProviderCredentials,
}

impl ProviderConfig {
    pub fn from_env() -> Self {
        Self {
            openai: ProviderCredentials {
                api_key: env_or_empty("OPENAI_API_KEY"),
                base_url: env_or_default("OPENAI_BASE_URL", "https://api.openai.com/v1"),
                api_version: None,
            },
            azure_openai: ProviderCredentials {
                api_key: env_or_empty("AZURE_OPENAI_API_KEY"),
                base_url: env_or_empty("AZURE_OPENAI_ENDPOINT"),
                api_version: env_optional("AZURE_OPENAI_API_VERSION")
                    .or_else(|| Some("2024-02-15-preview".to_string())),
            },
            anthropic: ProviderCredentials {
                api_key: env_or_empty("ANTHROPIC_API_KEY"),
                base_url: env_or_default("ANTHROPIC_BASE_URL", "https://api.anthropic.com"),
                api_version: env_optional("ANTHROPIC_API_VERSION")
                    .or_else(|| Some("2023-06-01".to_string())),
            },
            gemini: ProviderCredentials {
                api_key: env_or_empty("GEMINI_API_KEY"),
                base_url: env_or_default(
                    "GEMINI_BASE_URL",
                    "https://generativelanguage.googleapis.com/v1beta",
                ),
                api_version: None,
            },
            grok: ProviderCredentials {
                api_key: {
                    let grok = env_or_empty("GROK_API_KEY");
                    if grok.is_empty() {
                        env_or_empty("XAI_API_KEY")
                    } else {
                        grok
                    }
                },
                base_url: env_or_default("GROK_BASE_URL", "https://api.x.ai/v1"),
                api_version: None,
            },
            ollama: ProviderCredentials {
                api_key: env_or_empty("OLLAMA_API_KEY"),
                base_url: env_or_default("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
                api_version: None,
            },
        }
    }

    pub fn credentials_for(&self, provider: ProviderKind) -> Result<&ProviderCredentials> {
        let creds = match provider {
            ProviderKind::Openai => &self.openai,
            ProviderKind::AzureOpenai => &self.azure_openai,
            ProviderKind::Anthropic => &self.anthropic,
            ProviderKind::Gemini => &self.gemini,
            ProviderKind::Grok => &self.grok,
            ProviderKind::Ollama => &self.ollama,
        };

        if provider != ProviderKind::Ollama && creds.api_key.is_empty() {
            bail!(
                "Missing API key for provider '{}'. Set the corresponding environment variable.",
                provider.as_str()
            );
        }

        if provider == ProviderKind::AzureOpenai && creds.base_url.is_empty() {
            bail!("AZURE_OPENAI_ENDPOINT is required for azure_openai provider");
        }

        Ok(creds)
    }

    pub fn validate_for_providers(&self, providers: &[ProviderKind]) -> Result<()> {
        for provider in providers {
            self.credentials_for(*provider)
                .with_context(|| format!("Provider '{}' is not configured", provider.as_str()))?;
        }
        Ok(())
    }
}

fn env_or_empty(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}
