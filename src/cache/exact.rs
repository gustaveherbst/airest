use std::time::Duration;

use serde_json::Value;

use crate::cache::memory::CacheEntry;
use crate::cache::{
    effective_max_entries, scoped_exact_key, CacheLookup, CacheMeta, CacheStore,
};
use crate::definitions::{CacheConfig, EndpointDefinition};

pub async fn lookup(
    store: &CacheStore,
    _endpoint: &EndpointDefinition,
    config: &CacheConfig,
    input: &Value,
    scope_key: &str,
) -> anyhow::Result<CacheLookup> {
    let exclude = config.exclude_fields.as_deref().unwrap_or(&[]);
    let key = scoped_exact_key(scope_key, input, exclude);
    let started = std::time::Instant::now();

    let Some(entry) = store.get(&key) else {
        return Ok(CacheLookup::Miss);
    };

    let ttl = Duration::from_secs(config.ttl_seconds.unwrap_or(86_400));
    if entry.is_expired(ttl) {
        return Ok(CacheLookup::Miss);
    }

    Ok(CacheLookup::Hit {
        output: entry.output().clone(),
        cache_meta: CacheMeta {
            hit: true,
            similarity: Some(1.0),
            cached_request_id: Some(entry.cached_request_id().to_string()),
            latency_ms: started.elapsed().as_millis() as u64,
            estimated_tokens_saved: None,
        },
    })
}

pub async fn store(
    store: &CacheStore,
    endpoint: &EndpointDefinition,
    config: &CacheConfig,
    input: &Value,
    output: &Value,
    scope_key: &str,
) -> anyhow::Result<()> {
    let exclude = config.exclude_fields.as_deref().unwrap_or(&[]);
    let key = scoped_exact_key(scope_key, input, exclude);

    let max = effective_max_entries(config, store.default_max_entries());
    store.insert_scoped(
        scope_key,
        key,
        CacheEntry::new(output.clone(), format!("cache_{}", endpoint.name)),
        max,
    );
    Ok(())
}
