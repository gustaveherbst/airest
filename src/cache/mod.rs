mod embedder;
mod exact;
mod fingerprint;
mod memory;
mod semantic;
mod vector;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::AuthContext;
use crate::definitions::{CacheConfig, EndpointDefinition};

pub use embedder::{cosine_similarity, hash_embed, Embedder, EmbedderRegistry, HashEmbedder};
pub use fingerprint::endpoint_fingerprint;
pub use memory::CacheStore;
pub use vector::CacheStats;

/// Default cap on cached request/response signatures per endpoint scope.
pub const DEFAULT_MAX_ENTRIES_PER_SCOPE: usize = 50;

pub fn effective_max_entries(config: &CacheConfig, default: usize) -> usize {
    config.max_entries.unwrap_or(default).max(1)
}

pub fn scoped_exact_key(scope_key: &str, input: &Value, exclude: &[String]) -> String {
    format!("{scope_key}:{}", exact_cache_key(scope_key, input, exclude))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMeta {
    pub hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_request_id: Option<String>,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_tokens_saved: Option<u64>,
}

pub enum CacheLookup {
    Hit {
        output: Value,
        cache_meta: CacheMeta,
    },
    Miss,
}

impl CacheStore {
    pub async fn lookup(
        &self,
        endpoint: &EndpointDefinition,
        config: &CacheConfig,
        input: &Value,
        auth: Option<&AuthContext>,
    ) -> anyhow::Result<CacheLookup> {
        let scope_key = cache_scope_key(endpoint, config, auth);
        let mode = config.mode.as_deref().unwrap_or("exact");

        if mode == "semantic" {
            return semantic::lookup(self, endpoint, config, input, &scope_key).await;
        }

        exact::lookup(self, endpoint, config, input, &scope_key).await
    }

    pub async fn store(
        &self,
        endpoint: &EndpointDefinition,
        config: &CacheConfig,
        input: &Value,
        output: &Value,
        auth: Option<&AuthContext>,
    ) -> anyhow::Result<()> {
        let scope_key = cache_scope_key(endpoint, config, auth);
        let mode = config.mode.as_deref().unwrap_or("exact");

        if mode == "semantic" {
            return semantic::store(self, endpoint, config, input, output, &scope_key).await;
        }

        exact::store(self, endpoint, config, input, output, &scope_key).await
    }
}

pub fn normalized_input_json(input: &Value, exclude: &[String]) -> String {
    let mut value = input.clone();
    if let Some(obj) = value.as_object_mut() {
        for field in exclude {
            obj.remove(field);
        }
    }
    value.to_string()
}

pub fn exact_cache_key(scope: &str, input: &Value, exclude: &[String]) -> String {
    let normalized = normalized_input_json(input, exclude);
    let mut hasher = Sha256::new();
    hasher.update(scope.as_bytes());
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn cache_scope_key(
    endpoint: &EndpointDefinition,
    config: &CacheConfig,
    auth: Option<&AuthContext>,
) -> String {
    let scope = config.scope.as_deref().unwrap_or("endpoint");
    match scope {
        "tenant" => format!(
            "tenant:{}:{}:{}",
            auth.and_then(|a| a.tenant_id.clone()).unwrap_or_default(),
            endpoint.name,
            endpoint.version
        ),
        "global" => format!("global:{}:{}", endpoint.name, endpoint.version),
        _ => format!(
            "endpoint:{}:{}:{}:{}",
            endpoint.name,
            endpoint.version,
            endpoint.model.provider,
            endpoint.model.model
        ),
    }
}
