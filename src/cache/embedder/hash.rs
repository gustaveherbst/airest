use async_trait::async_trait;

use crate::cache::embedder::Embedder;

/// Built-in deterministic embedder (no external API). Suitable for dev/tests and offline mode.
#[derive(Debug, Clone)]
pub struct HashEmbedder {
    dimensions: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new(384)
    }
}

impl HashEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions: dimensions.max(64),
        }
    }
}

#[async_trait]
impl Embedder for HashEmbedder {
    fn name(&self) -> &str {
        "hash"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(hash_embed(text, self.dimensions))
    }
}

pub fn hash_embed(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dimensions];
    for token in text.split_whitespace() {
        let normalized = token.to_ascii_lowercase();
        let hash = normalized
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        let idx = (hash as usize) % dimensions;
        vec[idx] += 1.0;
    }
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}
