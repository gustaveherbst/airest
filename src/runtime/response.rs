use serde::Serialize;
use serde_json::Value;

use crate::cache::CacheMeta;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuccessResponse {
    pub success: bool,
    pub data: Value,
    pub meta: ResponseMeta,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMeta {
    pub request_id: String,
    pub endpoint: String,
    pub version: String,
    pub model: String,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheMeta>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorMeta {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl SuccessResponse {
    pub fn new(
        data: Value,
        request_id: String,
        endpoint: String,
        version: String,
        model: String,
        latency_ms: u64,
    ) -> Self {
        Self::new_with_cache(data, request_id, endpoint, version, model, latency_ms, None)
    }

    pub fn new_with_cache(
        data: Value,
        request_id: String,
        endpoint: String,
        version: String,
        model: String,
        latency_ms: u64,
        cache: Option<CacheMeta>,
    ) -> Self {
        Self {
            success: true,
            data,
            meta: ResponseMeta {
                request_id,
                endpoint,
                version,
                model,
                latency_ms,
                cache,
            },
        }
    }
}
