use std::sync::Arc;

use reqwest::Client;

use crate::errors::AiRestError;
use crate::llm::anthropic::complete_anthropic;
use crate::llm::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::llm::credentials::ProviderConfig;
use crate::llm::gemini::complete_gemini;
use crate::llm::openai_compatible::{complete_openai_compatible, AuthStyle};
use crate::llm::types::{LlmRequest, LlmResponse, ProviderKind};

#[derive(Clone)]
pub struct LlmRouter {
    client: Client,
    providers: ProviderConfig,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl LlmRouter {
    pub fn new(providers: ProviderConfig) -> Self {
        Self::with_circuit_breaker(providers, CircuitBreakerConfig::default())
    }

    pub fn with_circuit_breaker(providers: ProviderConfig, config: CircuitBreakerConfig) -> Self {
        Self {
            client: Client::new(),
            providers,
            circuit_breaker: Arc::new(CircuitBreaker::new(config)),
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub fn providers(&self) -> &ProviderConfig {
        &self.providers
    }

    pub async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, AiRestError> {
        self.circuit_breaker.check(request.provider)?;

        let creds = self
            .providers
            .credentials_for(request.provider)
            .map_err(|e| {
                AiRestError::with_details(
                    crate::errors::ErrorType::ModelProvider,
                    e.to_string(),
                    serde_json::json!({ "provider": request.provider.as_str() }),
                )
            })?;

        let result = match request.provider {
            ProviderKind::Openai => {
                complete_openai_compatible(
                    &self.client,
                    creds,
                    &request,
                    AuthStyle::Bearer,
                    true,
                    false,
                )
                .await
            }
            ProviderKind::AzureOpenai => {
                complete_openai_compatible(
                    &self.client,
                    creds,
                    &request,
                    AuthStyle::ApiKeyHeader,
                    true,
                    true,
                )
                .await
            }
            ProviderKind::Grok => {
                complete_openai_compatible(
                    &self.client,
                    creds,
                    &request,
                    AuthStyle::Bearer,
                    true,
                    false,
                )
                .await
            }
            ProviderKind::Ollama => {
                complete_openai_compatible(
                    &self.client,
                    creds,
                    &request,
                    AuthStyle::None,
                    false,
                    false,
                )
                .await
            }
            ProviderKind::Anthropic => complete_anthropic(&self.client, creds, &request).await,
            ProviderKind::Gemini => complete_gemini(&self.client, creds, &request).await,
        };

        match &result {
            Ok(_) => self.circuit_breaker.record_success(request.provider),
            Err(_) => self.circuit_breaker.record_failure(request.provider),
        }

        result
    }
}
