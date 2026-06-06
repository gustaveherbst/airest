use std::collections::HashSet;

use crate::errors::{AiRestError, ErrorType};

#[derive(Clone, Default)]
pub struct JtiDenylist {
    static_denied: HashSet<String>,
    #[cfg(feature = "auth-redis")]
    redis: Option<redis::aio::ConnectionManager>,
    #[allow(dead_code)]
    redis_key: String,
}

impl JtiDenylist {
    pub async fn from_config(config: &crate::config::Config) -> Self {
        let static_denied = config.jti_denylist.iter().cloned().collect();
        #[cfg(feature = "auth-redis")]
        let redis = if let Some(url) = config.redis_url.as_deref() {
            match redis::Client::open(url) {
                Ok(client) => redis::aio::ConnectionManager::new(client).await.ok(),
                Err(_) => None,
            }
        } else {
            None
        };

        Self {
            static_denied,
            #[cfg(feature = "auth-redis")]
            redis,
            redis_key: config.jti_denylist_redis_key.clone(),
        }
    }

    pub async fn is_denied(&self, jti: &str) -> Result<bool, AiRestError> {
        if self.static_denied.contains(jti) {
            return Ok(true);
        }

        #[cfg(feature = "auth-redis")]
        if let Some(redis) = &self.redis {
            use redis::AsyncCommands;
            let mut conn = redis.clone();
            let denied: bool = conn
                .sismember(&self.redis_key, jti)
                .await
                .unwrap_or(false);
            return Ok(denied);
        }

        Ok(false)
    }

    pub fn deny_token(&self) -> AiRestError {
        AiRestError::new(ErrorType::Authentication, "Token has been revoked.")
    }
}
