mod hash;
mod openai;

use async_trait::async_trait;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

pub use hash::{hash_embed, HashEmbedder};
pub use openai::OpenAiEmbedder;

use crate::config::Config;
use crate::definitions::CacheConfig;

#[async_trait]
pub trait Embedder: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;
}

#[derive(Clone)]
pub struct EmbedderRegistry {
    config: Config,
    cache: Arc<RwLock<HashMap<String, Arc<dyn Embedder>>>>,
}

impl EmbedderRegistry {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn embed(
        &self,
        cache_cfg: Option<&CacheConfig>,
        text: &str,
    ) -> anyhow::Result<Vec<f32>> {
        let embedder = self.resolve(cache_cfg)?;
        embedder.embed(text).await
    }

    fn resolve(&self, cache_cfg: Option<&CacheConfig>) -> anyhow::Result<Arc<dyn Embedder>> {
        let provider = resolve_provider(&self.config, cache_cfg);
        if let Ok(guard) = self.cache.read() {
            if let Some(existing) = guard.get(&provider) {
                return Ok(existing.clone());
            }
        }

        let embedder: Arc<dyn Embedder> = match provider.as_str() {
            "openai" => {
                let model = cache_cfg
                    .and_then(|c| c.embedder.as_ref())
                    .and_then(|e| e.model.as_deref())
                    .unwrap_or(&self.config.cache_embedder_model)
                    .to_string();
                Arc::new(OpenAiEmbedder::new(self.config.providers.clone(), model)?)
            }
            _ => Arc::new(HashEmbedder::default()),
        };

        if let Ok(mut guard) = self.cache.write() {
            guard.insert(provider, embedder.clone());
        }
        Ok(embedder)
    }
}

fn resolve_provider(config: &Config, cache_cfg: Option<&CacheConfig>) -> String {
    cache_cfg
        .and_then(|c| c.embedder.as_ref())
        .and_then(|e| e.provider.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if config.cache_embedder_provider.is_empty() {
                "hash".to_string()
            } else {
                config.cache_embedder_provider.clone()
            }
        })
        .to_ascii_lowercase()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).take(len).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().take(len).map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().take(len).map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    (dot / (na * nb)) as f64
}
