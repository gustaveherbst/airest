use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::cache::embedder::EmbedderRegistry;
use crate::cache::vector::VectorDatabase;
use crate::config::Config;
use crate::definitions::EndpointDefinition;

#[derive(Clone)]
pub struct CacheEntry {
    output: Value,
    cached_request_id: String,
    created_at: Instant,
}

#[derive(Clone)]
pub struct CacheStore {
    exact: Arc<RwLock<HashMap<String, CacheEntry>>>,
    pub vector: VectorDatabase,
    pub embedders: EmbedderRegistry,
    default_max_entries: usize,
}

impl CacheStore {
    pub fn new(config: &Config) -> Self {
        let store_path = resolve_store_path(config);
        let vector = VectorDatabase::open(store_path)
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "Vector cache persistence disabled; using memory");
                VectorDatabase::memory_only()
            });
        Self {
            exact: Arc::new(RwLock::new(HashMap::new())),
            vector,
            embedders: EmbedderRegistry::new(config.clone()),
            default_max_entries: config.cache_max_entries,
        }
    }

    pub fn default_max_entries(&self) -> usize {
        self.default_max_entries
    }

    pub fn get(&self, key: &str) -> Option<CacheEntry> {
        self.exact.read().ok()?.get(key).cloned()
    }

    pub fn insert_scoped(&self, scope_key: &str, key: String, entry: CacheEntry, max_entries: usize) {
        if let Ok(mut guard) = self.exact.write() {
            if !guard.contains_key(&key) {
                evict_oldest_in_scope(&mut guard, scope_key, max_entries);
            }
            guard.insert(key, entry);
        }
    }

    pub fn entries(&self) -> Vec<(String, CacheEntry)> {
        self.exact
            .read()
            .map(|guard| guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.exact.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn sync_endpoint(&self, endpoint: &EndpointDefinition) {
        let Some(cache) = endpoint.cache.as_ref() else {
            return;
        };
        if !cache.enabled {
            return;
        }
        let scope = super::cache_scope_key(endpoint, cache, None);
        let fingerprint = super::fingerprint::endpoint_fingerprint(endpoint);
        self.vector.set_scope_fingerprint(&scope, fingerprint);
    }

    pub fn stats(&self) -> crate::cache::vector::CacheStats {
        self.vector.stats(self.len())
    }
}

impl CacheEntry {
    pub fn new(output: Value, cached_request_id: String) -> Self {
        Self {
            output,
            cached_request_id,
            created_at: Instant::now(),
        }
    }

    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }

    pub fn output(&self) -> &Value {
        &self.output
    }

    pub fn cached_request_id(&self) -> &str {
        &self.cached_request_id
    }
}

fn resolve_store_path(config: &Config) -> Option<PathBuf> {
    if config.cache_store_path.as_os_str().is_empty() {
        None
    } else {
        Some(config.cache_store_path.clone())
    }
}

fn evict_oldest_in_scope(
    guard: &mut HashMap<String, CacheEntry>,
    scope_key: &str,
    max_entries: usize,
) {
    let prefix = format!("{scope_key}:");
    let scope_count = guard.keys().filter(|k| k.starts_with(&prefix)).count();
    if scope_count < max_entries {
        return;
    }
    let oldest_key = guard
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .min_by(|(ka, a), (kb, b)| a.created_at.cmp(&b.created_at).then(ka.cmp(kb)))
        .map(|(k, _)| k.clone());
    if let Some(key) = oldest_key {
        guard.remove(&key);
    }
}
