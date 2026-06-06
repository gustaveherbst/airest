#![cfg(feature = "auth-redis")]

use airest::auth::JtiDenylist;
use airest::config::Config;
use redis::AsyncCommands;

async fn redis_available(url: &str) -> bool {
    match redis::Client::open(url) {
        Ok(client) => client.get_multiplexed_async_connection().await.is_ok(),
        Err(_) => false,
    }
}

#[tokio::test]
async fn redis_jti_denylist_rejects_members_of_configured_set() {
    let url =
        std::env::var("AIREST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    if !redis_available(&url).await {
        eprintln!("Skipping redis jti test: no Redis at {url}");
        return;
    }

    let client = redis::Client::open(url.as_str()).expect("redis client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("redis connection");

    let key = format!("airest:jti:denylist:test:{}", uuid::Uuid::new_v4());
    let jti = format!("revoked-{}", uuid::Uuid::new_v4());
    let _: () = conn.sadd(&key, &jti).await.expect("sadd");

    let mut config = Config::for_test(None, std::path::PathBuf::from("api"));
    config.redis_url = Some(url);
    config.jti_denylist_redis_key = key.clone();

    let denylist = JtiDenylist::from_config(&config).await;
    assert!(denylist.is_denied(&jti).await.unwrap());
    assert!(!denylist.is_denied("still-valid-jti").await.unwrap());

    let _: () = conn.del(&key).await.expect("cleanup");
}
