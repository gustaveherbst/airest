use airest::llm::{parse_tool_api_name, tool_api_name};
use serde_json::json;

#[test]
fn tool_api_name_roundtrip() {
    let api = tool_api_name("support-kb", "search_tickets");
    assert_eq!(api, "support_kb__search_tickets");
    let (server, tool) = parse_tool_api_name(&api).unwrap();
    assert_eq!(server, "support-kb");
    assert_eq!(tool, "search-tickets");
}

#[test]
fn loads_kb_endpoint_with_tools() {
    use airest::definitions::{load_endpoint_definitions_with_options, LoadOptions};
    use std::path::Path;

    let loaded = load_endpoint_definitions_with_options(
        Path::new("./examples/mcp"),
        LoadOptions {
            recursive: false,
        },
    )
    .unwrap();
    let kb = loaded
        .iter()
        .find(|l| l.definition.name == "kb-ticket-search-hf")
        .expect("kb-ticket-search-hf");
    let tools = kb.definition.tools.as_ref().unwrap();
    assert!(tools.mcp_servers.as_ref().is_some_and(|s| !s.is_empty()));
    assert!(tools.allow.as_ref().is_some_and(|a| !a.is_empty()));
}

#[tokio::test]
async fn mock_mcp_server_lists_and_invokes_tool() {
    use airest::definitions::McpServerConfig;
    use airest::mcp::McpClient;

    let config = McpServerConfig {
        name: "support-kb".to_string(),
        transport: "stdio".to_string(),
        url: None,
        command: Some("node".to_string()),
        args: Some(vec![
            "./examples/support/mcp-mock-kb.mjs".to_string(),
        ]),
        env: None,
        headers: None,
    };

    let client = McpClient::connect(&config).await.unwrap();
    let tools = client.list_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "search_tickets"));

    let result = client
        .invoke_tool("search_tickets", json!({ "query": "password" }))
        .await
        .unwrap();
    assert!(result.to_string().contains("T-1001"));
}
