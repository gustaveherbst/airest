use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cache::embedder::Embedder;
use crate::llm::ProviderConfig;

#[derive(Clone)]
pub struct OpenAiEmbedder {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiEmbedder {
    pub fn new(providers: ProviderConfig, model: String) -> anyhow::Result<Self> {
        let creds = providers
            .credentials_for(crate::llm::ProviderKind::Openai)
            .map_err(|e| anyhow::anyhow!(e))?;
        if creds.api_key.is_empty() {
            anyhow::bail!("OpenAI embedder requires OPENAI_API_KEY");
        }
        Ok(Self {
            client: Client::new(),
            api_key: creds.api_key.clone(),
            base_url: creds.base_url.clone(),
            model,
        })
    }
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn name(&self) -> &str {
        "openai"
    }

    fn dimensions(&self) -> usize {
        0
    }

    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let body = EmbeddingRequest {
            model: self.model.clone(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let parsed: EmbeddingResponse = response.json().await?;
        let embedding = parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow::anyhow!("OpenAI embeddings response missing data"))?;
        Ok(embedding)
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
