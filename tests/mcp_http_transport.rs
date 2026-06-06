use airest::definitions::McpServerConfig;
use airest::mcp::McpClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn mcp_http_transport_lists_tools() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/mcp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "serverInfo": { "name": "mock", "version": "1.0.0" }
            }
        })))
        .up_to_n_times(1)
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/mcp"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [{
                    "name": "search",
                    "description": "Search KB",
                    "inputSchema": { "type": "object" }
                }]
            }
        })))
        .mount(&mock)
        .await;

    let config = McpServerConfig {
        name: "mock-kb".to_string(),
        transport: "http".to_string(),
        url: Some(format!("{}/mcp", mock.uri())),
        command: None,
        args: None,
        env: None,
        headers: None,
    };

    let client = McpClient::connect(&config).await.expect("connect http mcp");
    let tools = client.list_tools().await.expect("list tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "search");
}
