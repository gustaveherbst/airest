use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use crate::definitions::EndpointDefinition;

pub fn generate_openapi(endpoints: &[EndpointDefinition], base_url: &str) -> Value {
    let mut paths = serde_json::Map::new();

    for endpoint in endpoints {
        let operation = build_operation(endpoint);
        let method_key = endpoint.http_method().to_ascii_lowercase();

        let path_item = paths
            .entry(endpoint.path.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        path_item
            .as_object_mut()
            .expect("path item object")
            .insert(method_key, operation);
    }

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "aiREST API",
            "description": "Declarative AI REST endpoints generated from aiREST definitions.",
            "version": "1.0.0"
        },
        "servers": [{ "url": base_url }],
        "components": {
            "securitySchemes": build_security_schemes(endpoints)
        },
        "paths": paths
    })
}

fn build_security_schemes(endpoints: &[EndpointDefinition]) -> Map<String, Value> {
    let mut used = BTreeSet::new();
    for endpoint in endpoints {
        if endpoint.auth_required() {
            let auth_type = endpoint
                .auth
                .as_ref()
                .map(|auth| auth.auth_type())
                .unwrap_or("apiKey");
            used.insert(auth_type.to_string());
        }
    }

    let mut schemes = Map::new();
    if used.contains("apiKey") {
        schemes.insert(
            "ApiKeyAuth".to_string(),
            json!({
                "type": "apiKey",
                "in": "header",
                "name": "x-api-key"
            }),
        );
    }
    if used.contains("jwt") {
        schemes.insert(
            "BearerAuth".to_string(),
            json!({
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT"
            }),
        );
    }
    if used.contains("oauth2Introspect") {
        schemes.insert(
            "OAuth2Introspect".to_string(),
            json!({
                "type": "http",
                "scheme": "bearer",
                "description": "OAuth2 access token validated via RFC 7662 introspection"
            }),
        );
    }
    if used.contains("trustGateway") {
        schemes.insert(
            "TrustGatewayAuth".to_string(),
            json!({
                "type": "apiKey",
                "in": "header",
                "name": "x-user-id",
                "description": "Upstream gateway (Kong/Envoy) injects trusted identity headers such as x-user-id and x-tenant-id"
            }),
        );
    }
    schemes
}

fn operation_security(endpoint: &EndpointDefinition) -> Option<Value> {
    if !endpoint.auth_required() {
        return None;
    }
    let scheme = match endpoint.auth.as_ref().map(|auth| auth.auth_type()).unwrap_or("apiKey") {
        "jwt" => "BearerAuth",
        "oauth2Introspect" => "OAuth2Introspect",
        "trustGateway" => "TrustGatewayAuth",
        _ => "ApiKeyAuth",
    };
    Some(json!([{ scheme: [] }]))
}

fn build_operation(endpoint: &EndpointDefinition) -> Value {
    let mut operation = Map::new();
    operation.insert(
        "summary".to_string(),
        json!(endpoint.description.as_deref().unwrap_or(&endpoint.name)),
    );
    operation.insert("operationId".to_string(), json!(endpoint.name));

    if let Some(category) = &endpoint.category {
        operation.insert("tags".to_string(), json!([category]));
    }

    if let Some(security) = operation_security(endpoint) {
        operation.insert("security".to_string(), security);
    }

    if endpoint.is_get() {
        operation.insert(
            "parameters".to_string(),
            json!(query_parameters_from_schema(&endpoint.input_schema)),
        );
    } else {
        operation.insert(
            "requestBody".to_string(),
            json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": endpoint.input_schema,
                        "example": endpoint.examples.as_ref().and_then(|e| e.request.clone())
                    }
                }
            }),
        );
    }

    operation.insert(
        "responses".to_string(),
        json!({
            "200": {
                "description": "Successful aiREST execution",
                "content": {
                    "application/json": {
                        "schema": success_response_schema(&endpoint.output_schema),
                        "example": success_example(endpoint)
                    }
                }
            },
            "400": { "description": "Input validation error" },
            "401": { "description": "Authentication error" },
            "502": { "description": "Model provider or output validation error" }
        }),
    );

    Value::Object(operation)
}

fn query_parameters_from_schema(input_schema: &Value) -> Vec<Value> {
    let Some(properties) = input_schema
        .as_object()
        .and_then(|obj| obj.get("properties"))
        .and_then(|p| p.as_object())
    else {
        return Vec::new();
    };

    let required: Vec<&str> = input_schema
        .as_object()
        .and_then(|obj| obj.get("required"))
        .and_then(|r| r.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect()
        })
        .unwrap_or_default();

    properties
        .iter()
        .map(|(name, schema)| {
            json!({
                "name": name,
                "in": "query",
                "required": required.contains(&name.as_str()),
                "schema": schema,
                "example": schema_example(schema)
            })
        })
        .collect()
}

fn schema_example(schema: &Value) -> Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("integer") => json!(1),
        Some("number") => json!(1.0),
        Some("boolean") => json!(true),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => json!("example"),
    }
}

fn success_response_schema(output_schema: &Value) -> Value {
    json!({
        "type": "object",
        "required": ["success", "data", "meta"],
        "properties": {
            "success": { "type": "boolean", "enum": [true] },
            "data": output_schema,
            "meta": {
                "type": "object",
                "properties": {
                    "requestId": { "type": "string" },
                    "endpoint": { "type": "string" },
                    "version": { "type": "string" },
                    "model": { "type": "string" },
                    "latencyMs": { "type": "integer" }
                }
            }
        }
    })
}

fn success_example(endpoint: &EndpointDefinition) -> Value {
    let data = endpoint
        .examples
        .as_ref()
        .and_then(|examples| examples.response.clone())
        .unwrap_or_else(|| json!({}));

    json!({
        "success": true,
        "data": data,
        "meta": {
            "requestId": "req_example",
            "endpoint": endpoint.name,
            "version": endpoint.version,
            "model": endpoint.model.model,
            "latencyMs": 820
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::EndpointDefinition;
    use serde_json::json;

    fn sample_endpoint(method: &str) -> EndpointDefinition {
        let mut endpoint = crate::definitions::minimal_test_endpoint();
        endpoint.name = "sample".to_string();
        endpoint.method = method.to_string();
        endpoint.path = "/v1/sample".to_string();
        endpoint.input_schema = json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": { "type": "string" }
            }
        });
        endpoint.output_schema =
            json!({ "type": "object", "properties": { "result": { "type": "string" } } });
        endpoint
    }

    #[test]
    fn generates_get_operations_with_query_parameters() {
        let spec = generate_openapi(&[sample_endpoint("GET")], "http://localhost:3300");
        let get_op = &spec["paths"]["/v1/sample"]["get"];
        assert!(get_op["parameters"].is_array());
        assert!(get_op.get("requestBody").is_none());
    }

    #[test]
    fn generates_post_operations_with_request_body() {
        let spec = generate_openapi(&[sample_endpoint("POST")], "http://localhost:3300");
        let post_op = &spec["paths"]["/v1/sample"]["post"];
        assert!(post_op["requestBody"].is_object());
    }

    #[test]
    fn emits_auth_specific_security_schemes() {
        let mut endpoint = sample_endpoint("POST");
        endpoint.auth = Some(crate::definitions::AuthConfig {
            required: true,
            r#type: Some("jwt".to_string()),
            jwt: Some(Default::default()),
            oauth2: None,
            trust_gateway: None,
        });
        let spec = generate_openapi(&[endpoint], "http://localhost:3300");
        assert!(spec["components"]["securitySchemes"]["BearerAuth"].is_object());
        assert!(spec["paths"]["/v1/sample"]["post"]["security"][0]["BearerAuth"].is_array());
    }
}
