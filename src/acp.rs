use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

use crate::backend::{StreamEvent, UsageMetrics};

/// Standard Agent Client Protocol (ACP) JSON-RPC 2.0 Envelopes
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcNotification<T> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum JsonRpcResponse<R> {
    Success { jsonrpc: String, id: u64, result: R },
    Error { jsonrpc: String, id: u64, error: JsonRpcError },
}

static REQUEST_ID_GEN: AtomicU64 = AtomicU64::new(1);

impl<T> JsonRpcRequest<T> {
    pub fn new(method: &str, params: Option<T>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: REQUEST_ID_GEN.fetch_add(1, Ordering::Relaxed),
            method: method.to_string(),
            params,
        }
    }
}

impl<T> JsonRpcNotification<T> {
    pub fn new(method: &str, params: Option<T>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }
}

/// Model definition from ACP session/new
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpModelOption {
    pub value: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Thinking level option from ACP session/new
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpThinkingOption {
    pub value: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Config option descriptor returned by ACP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpConfigOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub options: Vec<serde_json::Value>,
}

/// ACP Initialize Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpInitializeResult {
    #[serde(default)]
    pub protocol_version: Option<u16>,
    #[serde(default)]
    pub agent_info: Option<serde_json::Value>,
}

/// ACP Session New Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSessionNewResult {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default, rename = "configOptions")]
    pub config_options: Vec<AcpConfigOption>,
    #[serde(default)]
    pub modes: Option<serde_json::Value>,
}

/// Resolves the pi-acp executable on the system.
pub fn resolve_pi_acp_binary() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        let candidates = [
            home_path.join(".local/share/mise/installs/node/lts/bin/pi-acp"),
            home_path.join(".local/bin/pi-acp"),
            home_path.join(".bun/bin/pi-acp"),
            home_path.join(".cargo/bin/pi-acp"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("pi-acp");
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    PathBuf::from("pi-acp")
}

/// Quick sync loader for Pi models from local config when ACP session is spinning up.
pub fn query_pi_models_sync() -> Vec<(String, String)> {
    let mut results = vec![("default".to_string(), "Default (System Preset)".to_string())];

    if let Ok(home) = std::env::var("HOME") {
        let models_path = PathBuf::from(&home).join(".pi/agent/models.json");
        if let Ok(content) = std::fs::read_to_string(&models_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(providers) = val.get("providers").and_then(|p| p.as_object()) {
                    for (provider_id, p_val) in providers {
                        if let Some(models) = p_val.get("models").and_then(|m| m.as_array()) {
                            for m in models {
                                if let Some(m_id) = m.get("id").and_then(|i| i.as_str()) {
                                    let m_name = m.get("name").and_then(|n| n.as_str()).unwrap_or(m_id);
                                    let qualified_id = if m_id.starts_with(provider_id) {
                                        m_id.to_string()
                                    } else {
                                        format!("{}/{}", provider_id, m_id)
                                    };
                                    results.push((qualified_id, format!("{}/{}", provider_id, m_name)));
                                }
                            }
                        }
                    }
                }
            }
        }

        let store_path = PathBuf::from(&home).join(".pi/agent/models-store.json");
        if let Ok(content) = std::fs::read_to_string(&store_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(providers) = val.as_object() {
                    for (provider_id, p_val) in providers {
                        if let Some(models) = p_val.get("models").and_then(|m| m.as_array()) {
                            for m in models {
                                if let Some(m_id) = m.get("id").and_then(|i| i.as_str()) {
                                    let m_name = m.get("name").and_then(|n| n.as_str()).unwrap_or(m_id);
                                    let qualified_id = format!("{}/{}", provider_id, m_id);
                                    if !results.iter().any(|(id, _)| id == &qualified_id) {
                                        results.push((qualified_id, format!("{}/{}", provider_id, m_name)));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if results.is_empty() {
        results.push(("default".to_string(), "Default (System Preset)".to_string()));
        results.push(("nvidia/deepseek-ai/deepseek-v4.1-flash".to_string(), "DeepSeek V4.1 Flash".to_string()));
        results.push(("openai/o3-mini".to_string(), "OpenAI o3-mini".to_string()));
        results.push(("agentrouter/gpt-6.1-sol".to_string(), "GPT-6.1 Sol".to_string()));
    }

    results
}

/// ACP Client managing a running pi-acp child process.
pub struct AcpClient {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    pub session_id: String,
    pub available_models: Vec<(String, String)>,
    pub available_thinking_levels: Vec<String>,
}

impl AcpClient {
    pub async fn spawn(cwd: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let acp_bin = resolve_pi_acp_binary();
        let mut cmd = Command::new(&acp_bin);

        // Prepend toolchain PATH
        let current_path = std::env::var("PATH").unwrap_or_default();
        if let Ok(home) = std::env::var("HOME") {
            let augmented_path = format!(
                "{}/.local/share/mise/installs/node/lts/bin:{}/.local/share/mise/shims:{}/.bun/bin:{}/.local/bin:{}/.cargo/bin:/usr/local/bin:{}",
                home, home, home, home, home, current_path
            );
            cmd.env("PATH", augmented_path);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().ok_or("Failed to open stdin for pi-acp")?;
        let stdout = child.stdout.take().ok_or("Failed to open stdout for pi-acp")?;

        let mut reader = BufReader::new(stdout).lines();

        // 1. Initialize ACP handshake
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": false,
                    "terminal": false
                }
            }
        });
        let mut init_line = serde_json::to_string(&init_req)?;
        init_line.push('\n');
        stdin.write_all(init_line.as_bytes()).await?;
        stdin.flush().await?;

        let init_resp_line = reader.next_line().await?.ok_or("pi-acp closed stdout during init")?;
        let _init_val: serde_json::Value = serde_json::from_str(&init_resp_line)?;

        // 2. Create ACP session
        let cwd_str = cwd.to_str().unwrap_or(".");
        let session_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session/new",
            "params": {
                "cwd": cwd_str,
                "mcpServers": []
            }
        });
        let mut session_line = serde_json::to_string(&session_req)?;
        session_line.push('\n');
        stdin.write_all(session_line.as_bytes()).await?;
        stdin.flush().await?;

        let session_resp_line = reader.next_line().await?.ok_or("pi-acp closed stdout during session/new")?;
        let session_val: serde_json::Value = serde_json::from_str(&session_resp_line)?;

        let session_result = session_val.get("result").ok_or("Invalid session/new result")?;
        let session_id = session_result
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("default-session")
            .to_string();

        let mut available_models = Vec::new();
        let mut available_thinking_levels = Vec::new();

        if let Some(config_opts) = session_result.get("configOptions").and_then(|c| c.as_array()) {
            for opt in config_opts {
                let opt_id = opt.get("id").and_then(|i| i.as_str()).unwrap_or("");
                if opt_id == "model" {
                    if let Some(options) = opt.get("options").and_then(|o| o.as_array()) {
                        for item in options {
                            if let Some(val) = item.get("value").and_then(|v| v.as_str()) {
                                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or(val);
                                available_models.push((val.to_string(), name.to_string()));
                            }
                        }
                    }
                } else if opt_id == "thought_level" {
                    if let Some(options) = opt.get("options").and_then(|o| o.as_array()) {
                        for item in options {
                            if let Some(val) = item.get("value").and_then(|v| v.as_str()) {
                                available_thinking_levels.push(val.to_string());
                            }
                        }
                    }
                }
            }
        }

        if available_models.is_empty() {
            available_models = query_pi_models_sync();
        }

        Ok(Self {
            child,
            stdin,
            session_id,
            available_models,
            available_thinking_levels,
        })
    }

    pub async fn set_model(&mut self, model: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": REQUEST_ID_GEN.fetch_add(1, Ordering::Relaxed),
            "method": "session/setConfigOption",
            "params": {
                "sessionId": self.session_id,
                "configId": "model",
                "value": model
            }
        });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn set_thinking_level(&mut self, level: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": REQUEST_ID_GEN.fetch_add(1, Ordering::Relaxed),
            "method": "session/setConfigOption",
            "params": {
                "sessionId": self.session_id,
                "configId": "thought_level",
                "value": level
            }
        });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn cancel(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {
                "sessionId": self.session_id
            }
        });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

/// Parses an incoming JSON line from pi-acp and converts it to a StreamEvent.
pub fn parse_acp_message(json_str: &str) -> Option<StreamEvent> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    // Check if it's a prompt response completing turn
    if val.get("result").is_some() {
        if let Some(res) = val.get("result") {
            if res.get("stopReason").is_some() {
                return Some(StreamEvent::Done);
            }
        }
    }

    // Check notification method
    let method = val.get("method").and_then(|m| m.as_str()).unwrap_or("");
    if method != "session/update" {
        return None;
    }

    let params = val.get("params")?;
    let update = params.get("update")?;
    let update_type = update.get("sessionUpdate").and_then(|u| u.as_str()).unwrap_or("");

    match update_type {
        "agent_message_chunk" | "text_delta" => {
            if let Some(content) = update.get("content") {
                if let Some(text) = content.get("text").and_then(|t| t.as_str()) {
                    return Some(StreamEvent::TextDelta(text.to_string()));
                }
            }
            if let Some(text) = update.get("text").and_then(|t| t.as_str()) {
                return Some(StreamEvent::TextDelta(text.to_string()));
            }
        }
        "thinking_delta" => {
            if let Some(text) = update.get("text").and_then(|t| t.as_str()) {
                return Some(StreamEvent::ThinkingDelta(text.to_string()));
            }
        }
        "tool_call" => {
            let id = update.get("toolCallId").and_then(|i| i.as_str()).unwrap_or("tool_call").to_string();
            let name = update.get("title")
                .or_else(|| update.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string();
            let args = update.get("rawInput")
                .or_else(|| update.get("args"))
                .map(|a| a.to_string())
                .unwrap_or_default();

            return Some(StreamEvent::ToolStart { id, name, args });
        }
        "tool_call_update" => {
            let id = update.get("toolCallId").and_then(|i| i.as_str()).unwrap_or("tool_call").to_string();
            let status = update.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status == "completed" {
                let result = update.get("rawOutput")
                    .map(|o| o.to_string())
                    .unwrap_or_default();
                return Some(StreamEvent::ToolEnd {
                    id,
                    name: "tool".to_string(),
                    result,
                    is_error: false,
                    duration_ms: 0,
                });
            }
        }
        "tool_result" => {
            let id = update.get("toolCallId").and_then(|i| i.as_str()).unwrap_or("tool_call").to_string();
            let name = update.get("title").and_then(|n| n.as_str()).unwrap_or("tool").to_string();
            let result = update.get("content")
                .or_else(|| update.get("rawOutput"))
                .map(|r| r.to_string())
                .unwrap_or_default();
            let is_error = update.get("isError").and_then(|b| b.as_bool()).unwrap_or(false);
            return Some(StreamEvent::ToolEnd {
                id,
                name,
                result,
                is_error,
                duration_ms: 0,
            });
        }
        "usage_update" => {
            let used = update.get("used").and_then(|u| u.as_u64()).unwrap_or(0);
            let _size = update.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            return Some(StreamEvent::Metrics(UsageMetrics {
                input_tokens: used,
                output_tokens: 0,
                total_tokens: used,
                duration_ms: 0,
            }));
        }
        "done" => {
            return Some(StreamEvent::Done);
        }
        "error" => {
            let message = update.get("message").and_then(|m| m.as_str()).unwrap_or("ACP error").to_string();
            return Some(StreamEvent::Error(message));
        }
        _ => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_acp_agent_message_chunk() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "sess_1",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "Hello from pi-acp!"
                    }
                }
            }
        }"#;

        let ev = parse_acp_message(json).expect("Must parse agent_message_chunk");
        assert_eq!(ev, StreamEvent::TextDelta("Hello from pi-acp!".to_string()));
    }

    #[test]
    fn test_parse_acp_tool_call() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "sess_1",
                "update": {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "call_123",
                    "title": "web_search",
                    "rawInput": { "queries": ["rust gtk4"] }
                }
            }
        }"#;

        let ev = parse_acp_message(json).expect("Must parse tool_call");
        match ev {
            StreamEvent::ToolStart { id, name, args } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "web_search");
                assert!(args.contains("rust gtk4"));
            }
            _ => panic!("Expected ToolStart"),
        }
    }

    #[test]
    fn test_parse_acp_tool_completed() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "sess_1",
                "update": {
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": "call_123",
                    "status": "completed",
                    "rawOutput": { "results": 5 }
                }
            }
        }"#;

        let ev = parse_acp_message(json).expect("Must parse tool completed");
        match ev {
            StreamEvent::ToolEnd { id, is_error, .. } => {
                assert_eq!(id, "call_123");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolEnd"),
        }
    }

    #[test]
    fn test_parse_acp_stop_reason_done() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "stopReason": "completed"
            }
        }"#;

        let ev = parse_acp_message(json).expect("Must parse stopReason completed");
        assert_eq!(ev, StreamEvent::Done);
    }

    #[test]
    fn test_query_pi_models_sync_not_empty() {
        let models = query_pi_models_sync();
        assert!(!models.is_empty(), "Model list must never be empty");
    }
}
