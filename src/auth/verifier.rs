use async_trait::async_trait;
use axum::http::HeaderMap;
use reqwest::Client;

use crate::auth::{AuthContext, JtiDenylist, JwksCache};
use crate::config::Config;
use crate::definitions::AuthConfig;
use crate::errors::AiRestError;

pub struct VerifyContext<'a> {
    pub config: &'a Config,
    pub auth: &'a AuthConfig,
    pub headers: &'a HeaderMap,
    pub jwks_cache: &'a JwksCache,
    pub http: &'a Client,
    pub jti_denylist: &'a JtiDenylist,
}

#[async_trait]
pub trait AuthVerifier: Send + Sync {
    fn strategy(&self) -> &'static str;

    async fn verify(&self, ctx: &VerifyContext<'_>) -> Result<Option<AuthContext>, AiRestError>;
}
