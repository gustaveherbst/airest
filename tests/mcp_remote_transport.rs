use std::net::{TcpListener, TcpStream};

use airest::definitions::McpServerConfig;
use airest::mcp::McpClient;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn start_mock_server(port: u16) -> tokio::process::Child {
    tokio::process::Command::new("node")
        .arg("./examples/mcp/mcp-mock-kb-remote.mjs")
        .env("MCP_PORT", port.to_string())
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn mock MCP server")
}

async fn wait_for_mock(port: u16) {
    for _ in 0..40 {
        if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("mock MCP server did not start on port {port}");
}

#[tokio::test]
async fn mcp_http_remote_server_lists_tools() {
    let port = free_port();
    let mut child = start_mock_server(port).await;
    wait_for_mock(port).await;

    let config = McpServerConfig {
        name: "support-kb".to_string(),
        transport: "http".to_string(),
        url: Some(format!("http://127.0.0.1:{port}/mcp")),
        command: None,
        args: None,
        env: None,
        headers: None,
    };

    let client = McpClient::connect(&config).await.expect("http connect");
    let tools = client.list_tools().await.expect("list tools");
    assert!(tools.iter().any(|t| t.name == "search_tickets"));

    let _ = child.kill().await;
}

#[tokio::test]
async fn mcp_sse_remote_server_lists_tools() {
    let port = free_port();
    let mut child = start_mock_server(port).await;
    wait_for_mock(port).await;

    let config = McpServerConfig {
        name: "support-kb".to_string(),
        transport: "sse".to_string(),
        url: Some(format!("http://127.0.0.1:{port}/mcp/sse")),
        command: None,
        args: None,
        env: None,
        headers: None,
    };

    let client = McpClient::connect(&config).await.expect("sse connect");
    let tools = client.list_tools().await.expect("list tools");
    assert!(tools.iter().any(|t| t.name == "search_tickets"));

    let _ = child.kill().await;
}
