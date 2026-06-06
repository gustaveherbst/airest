use airest::cache::{cosine_similarity, endpoint_fingerprint, CacheStore, Embedder, HashEmbedder};
use airest::cache::hash_embed;
use airest::config::Config;
use airest::definitions::{minimal_test_endpoint, CacheConfig};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn hash_embedder_produces_similar_vectors_for_similar_text() {
    let a = hash_embed("hello world from airest", 384);
    let b = hash_embed("hello  world   from airest", 384);
    let c = hash_embed("completely different topic about finance", 384);
    assert!(cosine_similarity(&a, &b) > 0.99);
    assert!(cosine_similarity(&a, &c) < 0.3);
}

#[test]
fn hash_embedder_default_dimensions() {
    let embedder = HashEmbedder::default();
    assert_eq!(embedder.dimensions(), 384);
}

#[tokio::test]
async fn semantic_cache_hits_on_near_duplicate_input() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(CacheConfig {
        enabled: true,
        mode: Some("semantic".to_string()),
        similarity_threshold: Some(0.75),
        ttl_seconds: Some(3600),
        max_entries: Some(100),
        scope: None,
        exclude_fields: None,
        embedder: None,
        store: None,
        bypass_on_guardrail_block: None,
    });

    let config = Config::for_test(None, std::path::PathBuf::from("api"));
    let store = CacheStore::new(&config);
    let cache_cfg = endpoint.cache.as_ref().unwrap();

    let input = json!({ "text": "What is the capital of France?" });
    let output = json!({ "answer": "Paris" });

    store
        .store(&endpoint, cache_cfg, &input, &output, None)
        .await
        .unwrap();

    let near = json!({ "text": "what is the capital of france" });
    let lookup = store
        .lookup(&endpoint, cache_cfg, &near, None)
        .await
        .unwrap();
    assert!(matches!(lookup, airest::cache::CacheLookup::Hit { .. }));
}

#[test]
fn endpoint_fingerprint_changes_when_prompt_changes() {
    let mut endpoint = minimal_test_endpoint();
    let fp1 = endpoint_fingerprint(&endpoint);
    endpoint.system_prompt = "You are a different assistant.".to_string();
    let fp2 = endpoint_fingerprint(&endpoint);
    assert_ne!(fp1, fp2);
}

#[tokio::test]
async fn semantic_cache_caps_entries_per_endpoint() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(CacheConfig {
        enabled: true,
        mode: Some("semantic".to_string()),
        similarity_threshold: Some(0.75),
        ttl_seconds: Some(3600),
        max_entries: Some(2),
        scope: None,
        exclude_fields: None,
        embedder: None,
        store: None,
        bypass_on_guardrail_block: None,
    });

    let config = Config::for_test(None, std::path::PathBuf::from("api"));
    let store = CacheStore::new(&config);
    let cache_cfg = endpoint.cache.as_ref().unwrap();

    let inputs = [
        json!({ "text": "first query" }),
        json!({ "text": "second query" }),
        json!({ "text": "third query" }),
    ];
    for (index, input) in inputs.iter().enumerate() {
        store
            .store(
                &endpoint,
                cache_cfg,
                input,
                &json!({ "n": index }),
                None,
            )
            .await
            .unwrap();
        if index < inputs.len() - 1 {
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
    }

    assert_eq!(store.stats().vector_entries, 2);
    let first_lookup = store
        .lookup(&endpoint, cache_cfg, &inputs[0], None)
        .await
        .unwrap();
    assert!(matches!(first_lookup, airest::cache::CacheLookup::Miss));
}

#[tokio::test]
async fn vector_cache_persists_across_store_instances() {
    let dir = TempDir::new().unwrap();
    let store_path = dir.path().join("cache.redb");

    let mut config = Config::for_test(None, std::path::PathBuf::from("api"));
    config.cache_store_path = store_path.clone();

    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(CacheConfig {
        enabled: true,
        mode: Some("semantic".to_string()),
        similarity_threshold: Some(0.75),
        ttl_seconds: Some(3600),
        max_entries: Some(100),
        scope: None,
        exclude_fields: None,
        embedder: None,
        store: None,
        bypass_on_guardrail_block: None,
    });
    let cache_cfg = endpoint.cache.as_ref().unwrap();
    let input = json!({ "text": "persist me" });
    let output = json!({ "ok": true });

    {
        let store = CacheStore::new(&config);
        store
            .store(&endpoint, cache_cfg, &input, &output, None)
            .await
            .unwrap();
        assert_eq!(store.stats().vector_entries, 1);
    }

    let store2 = CacheStore::new(&config);
    assert_eq!(store2.stats().vector_entries, 1);
    let lookup = store2
        .lookup(&endpoint, cache_cfg, &input, None)
        .await
        .unwrap();
    assert!(matches!(lookup, airest::cache::CacheLookup::Hit { .. }));
}
