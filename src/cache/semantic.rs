use serde_json::Value;
use uuid::Uuid;

use super::exact;
use crate::cache::fingerprint::endpoint_fingerprint;
use crate::cache::vector::new_record;
use crate::cache::{
    effective_max_entries, exact_cache_key, normalized_input_json, CacheLookup, CacheMeta,
    CacheStore,
};
use crate::definitions::{CacheConfig, EndpointDefinition};

pub async fn lookup(
    store: &CacheStore,
    endpoint: &EndpointDefinition,
    config: &CacheConfig,
    input: &Value,
    scope_key: &str,
) -> anyhow::Result<CacheLookup> {
    let threshold = config.similarity_threshold.unwrap_or(0.92);
    let exclude = config.exclude_fields.as_deref().unwrap_or(&[]);
    let query_text = normalized_input_json(input, exclude);
    let query_vec = store.embedders.embed(Some(config), &query_text).await?;
    let started = std::time::Instant::now();
    let ttl = config.ttl_seconds.unwrap_or(86_400);
    let fingerprint = endpoint_fingerprint(endpoint);

    store
        .vector
        .set_scope_fingerprint(scope_key, fingerprint.clone());

    if let Some((record, similarity)) = store.vector.search(
        scope_key,
        &fingerprint,
        &query_vec,
        threshold,
        ttl,
    ) {
        return Ok(CacheLookup::Hit {
            output: record.output.clone(),
            cache_meta: CacheMeta {
                hit: true,
                similarity: Some(similarity),
                cached_request_id: Some(record.cached_request_id),
                latency_ms: started.elapsed().as_millis() as u64,
                estimated_tokens_saved: estimate_tokens_saved(&record.output),
            },
        });
    }

    exact::lookup(store, endpoint, config, input, scope_key).await
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
    let text = normalized_input_json(input, exclude);
    let embedding = store.embedders.embed(Some(config), &text).await?;
    let key = format!("{}:{}", scope_key, exact_cache_key(scope_key, input, exclude));
    let fingerprint = endpoint_fingerprint(endpoint);

    store
        .vector
        .set_scope_fingerprint(scope_key, fingerprint.clone());

    store.vector.insert(
        new_record(
            key,
            scope_key.to_string(),
            fingerprint,
            embedding,
            output.clone(),
            format!("cache_{}", Uuid::new_v4().simple()),
        ),
        effective_max_entries(config, store.default_max_entries()),
    )?;

    Ok(())
}

fn estimate_tokens_saved(output: &Value) -> Option<u64> {
    let chars = output.to_string().len();
    Some((chars / 4).max(1) as u64)
}
