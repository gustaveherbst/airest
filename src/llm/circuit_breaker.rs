use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::errors::{AiRestError, ErrorType};
use crate::llm::ProviderKind;

#[derive(Debug)]
struct ProviderCircuit {
    consecutive_failures: AtomicU32,
    opened_at: Mutex<Option<Instant>>,
}

impl ProviderCircuit {
    fn new() -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            opened_at: Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub reset_after: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_after: Duration::from_secs(30),
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    providers: [ProviderCircuit; 6],
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            providers: std::array::from_fn(|_| ProviderCircuit::new()),
        }
    }

    pub fn check(&self, provider: ProviderKind) -> Result<(), AiRestError> {
        let circuit = self.circuit(provider);
        let opened_at = circuit.opened_at.lock().ok().and_then(|g| *g);
        let Some(opened) = opened_at else {
            return Ok(());
        };

        if opened.elapsed() >= self.config.reset_after {
            if let Ok(mut guard) = circuit.opened_at.lock() {
                *guard = None;
            }
            circuit.consecutive_failures.store(0, Ordering::Relaxed);
            return Ok(());
        }

        Err(AiRestError::with_details(
            ErrorType::ModelProvider,
            "LLM provider circuit breaker is open due to repeated failures.",
            serde_json::json!({
                "provider": provider.as_str(),
                "retryAfterSeconds": self.config.reset_after.as_secs(),
            }),
        ))
    }

    pub fn record_success(&self, provider: ProviderKind) {
        let circuit = self.circuit(provider);
        circuit.consecutive_failures.store(0, Ordering::Relaxed);
        if let Ok(mut guard) = circuit.opened_at.lock() {
            *guard = None;
        }
    }

    pub fn record_failure(&self, provider: ProviderKind) {
        let circuit = self.circuit(provider);
        let failures = circuit.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= self.config.failure_threshold {
            if let Ok(mut guard) = circuit.opened_at.lock() {
                if guard.is_none() {
                    *guard = Some(Instant::now());
                }
            }
        }
    }

    pub fn is_open(&self, provider: ProviderKind) -> bool {
        self.check(provider).is_err()
    }

    fn circuit(&self, provider: ProviderKind) -> &ProviderCircuit {
        &self.providers[provider_index(provider)]
    }
}

fn provider_index(provider: ProviderKind) -> usize {
    match provider {
        ProviderKind::Openai => 0,
        ProviderKind::AzureOpenai => 1,
        ProviderKind::Anthropic => 2,
        ProviderKind::Gemini => 3,
        ProviderKind::Grok => 4,
        ProviderKind::Ollama => 5,
    }
}
