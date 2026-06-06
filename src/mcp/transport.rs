use std::collections::HashMap;

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, oneshot};

use crate::definitions::McpServerConfig;
use crate::errors::{AiRestError, ErrorType};

#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}

pub enum McpTransport {
    Stdio(StdioTransport),
    Http(HttpTransport),
    Sse(SseTransport),
}

impl McpTransport {
    pub async fn connect(config: &McpServerConfig, http: &Client) -> Result<Self, AiRestError> {
        match config.transport.as_str() {
            "stdio" => Ok(Self::Stdio(StdioTransport::connect(config).await?)),
            "http" | "streamableHttp" => {
                Ok(Self::Http(HttpTransport::connect(config, http.clone())?))
            }
            "sse" => Ok(Self::Sse(SseTransport::connect(config, http.clone()).await?)),
            other => Err(AiRestError::with_details(
                ErrorType::McpTool,
                "Unsupported MCP transport.",
                serde_json::json!({ "transport": other }),
            )),
        }
    }

    pub async fn initialize(&self) -> Result<(), AiRestError> {
        let _ = self
            .call(
                "initialize",
                Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "airest", "version": "0.1.0" }
                })),
            )
            .await?;
        Ok(())
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, AiRestError> {
        match self {
            Self::Stdio(t) => t.call(method, params).await,
            Self::Http(t) => t.call(method, params).await,
            Self::Sse(t) => t.call(method, params).await,
        }
    }
}

pub struct StdioTransport {
    stdin: std::sync::Arc<Mutex<Option<ChildStdin>>>,
    stdout: std::sync::Arc<Mutex<BufReader<ChildStdout>>>,
    next_id: std::sync::Arc<Mutex<u64>>,
}

impl StdioTransport {
    pub async fn connect(config: &McpServerConfig) -> Result<Self, AiRestError> {
        let command = config.command.as_ref().ok_or_else(|| {
            AiRestError::new(
                ErrorType::McpTool,
                "MCP stdio transport requires command.",
            )
        })?;

        let mut cmd = Command::new(command);
        if let Some(args) = &config.args {
            cmd.args(args);
        }
        if let Some(env) = &config.env {
            cmd.envs(env);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn().map_err(|e| {
            AiRestError::with_details(
                ErrorType::McpTool,
                "Failed to start MCP server process.",
                serde_json::json!({ "reason": e.to_string() }),
            )
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            AiRestError::new(ErrorType::McpTool, "MCP process stdin unavailable.")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AiRestError::new(ErrorType::McpTool, "MCP process stdout unavailable.")
        })?;

        Ok(Self {
            stdin: std::sync::Arc::new(Mutex::new(Some(stdin))),
            stdout: std::sync::Arc::new(Mutex::new(BufReader::new(stdout))),
            next_id: std::sync::Arc::new(Mutex::new(1)),
        })
    }

    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, AiRestError> {
        let id = next_request_id(&self.next_id).await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let line = serde_json::to_string(&request).map_err(|_| internal_mcp())?;
        {
            let mut guard = self.stdin.lock().await;
            let stdin = guard.as_mut().ok_or_else(|| internal_mcp())?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|_| internal_mcp())?;
            stdin.write_all(b"\n").await.map_err(|_| internal_mcp())?;
        }

        read_json_rpc_response(&self.stdout, Some(id)).await
    }
}

pub struct HttpTransport {
    http: Client,
    url: String,
    headers: HashMap<String, String>,
    next_id: std::sync::Arc<Mutex<u64>>,
    session_id: std::sync::Arc<Mutex<Option<String>>>,
}

impl HttpTransport {
    pub fn connect(config: &McpServerConfig, http: Client) -> Result<Self, AiRestError> {
        let url = config.url.as_ref().ok_or_else(|| {
            AiRestError::new(
                ErrorType::McpTool,
                "MCP HTTP transport requires url.",
            )
        })?;

        Ok(Self {
            http,
            url: url.clone(),
            headers: crate::mcp::env::expand_mcp_headers(config.headers.clone()),
            next_id: std::sync::Arc::new(Mutex::new(1)),
            session_id: std::sync::Arc::new(Mutex::new(None)),
        })
    }

    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, AiRestError> {
        let id = next_request_id(&self.next_id).await;
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut builder = self
            .http
            .post(&self.url)
            .json(&request)
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        if method != "initialize" {
            if let Some(session) = self.session_id.lock().await.as_ref() {
                builder = builder.header("mcp-session-id", session);
            }
        }

        let response = builder.send().await.map_err(|_| internal_mcp())?;
        if method == "initialize" {
            if let Some(session) = response
                .headers()
                .get("mcp-session-id")
                .and_then(|v| v.to_str().ok())
            {
                *self.session_id.lock().await = Some(session.to_string());
            }
        }

        let body: JsonRpcResponse = response.json().await.map_err(|_| internal_mcp())?;
        json_rpc_value(body, Some(id))
    }
}

pub struct SseTransport {
    http: Client,
    message_url: String,
    headers: HashMap<String, String>,
    next_id: std::sync::Arc<Mutex<u64>>,
    responses: std::sync::Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, AiRestError>>>>>,
}

impl SseTransport {
    pub async fn connect(config: &McpServerConfig, http: Client) -> Result<Self, AiRestError> {
        let sse_url = config.url.as_ref().ok_or_else(|| {
            AiRestError::new(ErrorType::McpTool, "MCP SSE transport requires url.")
        })?;

        let headers = crate::mcp::env::expand_mcp_headers(config.headers.clone());
        let responses =
            std::sync::Arc::new(Mutex::new(HashMap::<u64, oneshot::Sender<_>>::new()));

        let mut request = http.get(sse_url).header("Accept", "text/event-stream");
        for (key, value) in &headers {
            request = request.header(key, value);
        }

        let response = request.send().await.map_err(|_| internal_mcp())?;
        if !response.status().is_success() {
            return Err(internal_mcp());
        }

        let base_url = sse_url.trim_end_matches('/');
        let mut stream = response.bytes_stream();
        let mut current_event = String::new();
        let mut current_data = String::new();
        let mut message_url: Option<String> = None;

        while message_url.is_none() {
            let chunk = stream
                .next()
                .await
                .ok_or_else(internal_mcp)?
                .map_err(|_| internal_mcp())?;
            for line in String::from_utf8_lossy(&chunk).lines() {
                if line.is_empty() {
                    if current_event == "endpoint" && !current_data.is_empty() {
                        message_url = Some(resolve_message_url(base_url, &current_data));
                    }
                    current_event.clear();
                    current_data.clear();
                    continue;
                }
                if let Some(value) = line.strip_prefix("event:") {
                    current_event = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    if !current_data.is_empty() {
                        current_data.push('\n');
                    }
                    current_data.push_str(value.trim());
                }
            }
        }

        let message_url = message_url.ok_or_else(internal_mcp)?;
        let responses_reader = responses.clone();
        tokio::spawn(async move {
            let mut current_event = String::new();
            let mut current_data = String::new();
            while let Some(chunk) = stream.next().await {
                let Ok(chunk) = chunk else { break };
                for line in String::from_utf8_lossy(&chunk).lines() {
                    if line.is_empty() {
                        if current_event == "message" && !current_data.is_empty() {
                            if let Ok(parsed) =
                                serde_json::from_str::<JsonRpcResponse>(&current_data)
                            {
                                if let Some(id) = parsed.id {
                                    if let Some(sender) =
                                        responses_reader.lock().await.remove(&id)
                                    {
                                        let _ = sender.send(json_rpc_value(parsed, Some(id)));
                                    }
                                }
                            }
                        }
                        current_event.clear();
                        current_data.clear();
                        continue;
                    }
                    if let Some(value) = line.strip_prefix("event:") {
                        current_event = value.trim().to_string();
                    } else if let Some(value) = line.strip_prefix("data:") {
                        if !current_data.is_empty() {
                            current_data.push('\n');
                        }
                        current_data.push_str(value.trim());
                    }
                }
            }
        });

        Ok(Self {
            http,
            message_url,
            headers,
            next_id: std::sync::Arc::new(Mutex::new(1)),
            responses,
        })
    }

    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, AiRestError> {
        let id = next_request_id(&self.next_id).await;
        let (tx, rx) = oneshot::channel();
        self.responses.lock().await.insert(id, tx);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut builder = self.http.post(&self.message_url).json(&request);
        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }
        builder.send().await.map_err(|_| internal_mcp())?;

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            _ => {
                self.responses.lock().await.remove(&id);
                Err(internal_mcp())
            }
        }
    }
}

fn resolve_message_url(base: &str, data: &str) -> String {
    if data.starts_with("http://") || data.starts_with("https://") {
        return data.to_string();
    }
    let origin = base_url_origin(base).unwrap_or_else(|| base.trim_end_matches('/').to_string());
    if data.starts_with('/') {
        format!("{origin}{data}")
    } else {
        format!("{origin}/{data}")
    }
}

fn base_url_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split('/').next()?;
    Some(format!("{scheme}://{host}"))
}

async fn next_request_id(counter: &std::sync::Arc<Mutex<u64>>) -> u64 {
    let mut guard = counter.lock().await;
    let current = *guard;
    *guard += 1;
    current
}

async fn read_json_rpc_response(
    stdout: &std::sync::Arc<Mutex<BufReader<ChildStdout>>>,
    expected_id: Option<u64>,
) -> Result<Value, AiRestError> {
    let mut response_line = String::new();
    {
        let mut reader = stdout.lock().await;
        reader
            .read_line(&mut response_line)
            .await
            .map_err(|_| internal_mcp())?;
    }

    let response: JsonRpcResponse =
        serde_json::from_str(&response_line).map_err(|_| internal_mcp())?;
    json_rpc_value(response, expected_id)
}

fn json_rpc_value(response: JsonRpcResponse, expected_id: Option<u64>) -> Result<Value, AiRestError> {
    if let Some(expected) = expected_id {
        if response.id != Some(expected) {
            return Err(internal_mcp());
        }
    }
    if let Some(error) = response.error {
        return Err(AiRestError::with_details(
            ErrorType::McpTool,
            "MCP tool call failed.",
            error,
        ));
    }
    response.result.ok_or_else(internal_mcp)
}

fn internal_mcp() -> AiRestError {
    AiRestError::new(ErrorType::McpTool, "MCP communication error.")
}
