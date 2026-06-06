use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthContext {
    pub subject: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing)]
    pub raw_claims: Option<Value>,
}

impl AuthContext {
    pub fn anonymous() -> Self {
        Self {
            subject: None,
            tenant_id: None,
            scopes: vec![],
            raw_claims: None,
        }
    }
}
