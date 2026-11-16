//! Minimal Agent Client Protocol (ACP) client for `agy acp`.
//!
//! The agent runs as one long-lived child process per conversation, speaking
//! JSON-RPC 2.0 over stdio. Three distinct message shapes arrive interleaved on
//! stdout, and conflating them is what breaks naive clients:
//!
//! 1. **Notifications** — `session/update`, no `id`. Stream to the UI.
//! 2. **Server→client requests** — `session/request_permission`, has `id` and
//!    `method`. Must be *answered*, or the agent blocks forever.
//! 3. **Responses** — has `id`, no `method`. Match it against the request we
//!    are waiting on; anything else belongs to an earlier call.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::backend::{PermissionRequest, SessionConfig, StreamEvent, UsageMetrics};

static REQUEST_ID_GEN: AtomicU64 = AtomicU64::new(1);

/// Protocol version advertised during `initialize`.
pub const PROTOCOL_VERSION: u16 = 1;

/// Resolves the ACP agent binary: configured path, `$PATH`, common user bin dirs, else `agy`.
pub fn resolve_acp_binary(configured: &str) -> PathBuf {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_file() || path.is_absolute() {
            return path;
        }
        if let Ok(paths) = std::env::var("PATH") {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join(configured);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);
            for rel in [".local/bin", ".opencode/bin", ".bun/bin", ".cargo/bin"] {
                let candidate = home_path.join(rel).join(configured);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        return path;
    }
    PathBuf::from("agy")
}

/// Splits an incoming line into the three protocol message shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpMessage {
    /// Server→client request that must be answered.
    Request { id: u64, method: String, params: serde_json::Value },
    /// `session/update` style notification.
    Notification { method: String, params: serde_json::Value },
    /// Response to one of our requests.
    Response { id: u64, result: serde_json::Value },
    /// Error response to one of our requests.
    ErrorResponse { id: u64, message: String },
}

/// Classifies one JSON line. Returns `None` when the line is not JSON-RPC.
pub fn classify_message(line: &str) -> Option<AcpMessage> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    let id = val.get("id").and_then(|v| v.as_u64());
    let method = val.get("method").and_then(|m| m.as_str());
    let params = val.get("params").cloned().unwrap_or(serde_json::Value::Null);

    match (id, method) {
        // Server→client request: has both id and method.
        (Some(id), Some(method)) => Some(AcpMessage::Request {
            id,
            method: method.to_string(),
            params,
        }),
        // Notification: method only.
        (None, Some(method)) => Some(AcpMessage::Notification {
            method: method.to_string(),
            params,
        }),
        // Response: id only.
        (Some(id), None) => {
            if let Some(err) = val.get("error") {
                let message = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("ACP error")
                    .to_string();
                Some(AcpMessage::ErrorResponse { id, message })
            } else {
                Some(AcpMessage::Response {
                    id,
                    result: val.get("result").cloned().unwrap_or(serde_json::Value::Null),
                })
            }
        }
        (None, None) => None,
    }
}

/// Entry of a `configOptions` catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigOption {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    options: Vec<serde_json::Value>,
    #[serde(default, rename = "currentValue")]
    current_value: Option<String>,
}

/// Extracts `(value, label)` pairs from a `value`/`name` keyed option list.
fn option_pairs(options: &[serde_json::Value]) -> Vec<(String, String)> {
    options
        .iter()
        .filter_map(|o| {
            let value = o.get("value").and_then(|v| v.as_str())?;
            let label = o
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(value);
            Some((value.to_string(), label.to_string()))
        })
        .collect()
}

/// Reads model / mode / thinking catalogs out of a `session/new` result.
pub fn parse_session_config(result: &serde_json::Value) -> SessionConfig {
    let mut config = SessionConfig {
        current_model: result
            .get("currentModelId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        current_mode: result
            .get("currentModeId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        ..Default::default()
    };

    if let Some(models) = result.get("models") {
        if let Some(available) = models.get("availableModels").and_then(|m| m.as_array()) {
            config.models = available
                .iter()
                .filter_map(|m| {
                    let id = m
                        .get("modelId")
                        .or_else(|| m.get("value"))
                        .and_then(|v| v.as_str())?;
                    let name = m
                        .get("name")
                        .or_else(|| m.get("displayName"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(id);
                    Some((id.to_string(), name.to_string()))
                })
                .collect();
        }
        if config.current_model.is_empty() {
            config.current_model = models
                .get("currentModelId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
        }
    }

    if let Some(modes) = result.get("modes") {
        if let Some(available) = modes.get("availableModes").and_then(|m| m.as_array()) {
            config.modes = available
                .iter()
                .filter_map(|m| {
                    let id = m.get("id").or_else(|| m.get("value")).and_then(|v| v.as_str())?;
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                    Some((id.to_string(), name.to_string()))
                })
                .collect();
        }
        if config.current_mode.is_empty() {
            config.current_mode = modes
                .get("currentModeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
        }
    }

    if let Some(opts) = result.get("configOptions").and_then(|c| c.as_array()) {
        for opt in opts {
            let Ok(opt) = serde_json::from_value::<ConfigOption>(opt.clone()) else {
                continue;
            };
            match opt.id.as_str() {
                "model" => {
                    if config.models.is_empty() {
                        config.models = option_pairs(&opt.options);
                    }
                    if config.current_model.is_empty() {
                        config.current_model = opt.current_value.unwrap_or_default();
                    }
                }
                "mode" => {
                    if config.modes.is_empty() {
                        config.modes = option_pairs(&opt.options);
                    }
                    if config.current_mode.is_empty() {
                        config.current_mode = opt.current_value.unwrap_or_default();
                    }
                }
                "thought_level" | "thinking_level" | "reasoning_effort" => {
                    config.thinking_levels =
                        option_pairs(&opt.options).into_iter().map(|(v, _)| v).collect();
                }
                _ => {}
            }
        }
    }

    let _ = &mut config.modes;
    config
}

/// Builds the prompt content blocks: images inline, other files as `@` links.
pub fn build_prompt_blocks(
    prompt: &str,
    attached_files: &[(String, String)],
) -> Vec<serde_json::Value> {
    let mut blocks = Vec::new();

    for (name, content) in attached_files {
        if crate::capture::is_image_path(name) {
            let mime = crate::capture::image_mime_type(name);
            let data = image_base64(name, content);
            if data.is_empty() {
                continue;
            }
            blocks.push(serde_json::json!({
                "type": "image",
                "mimeType": mime,
                "data": data,
            }));
        } else if Path::new(name).is_file() {
            blocks.push(serde_json::json!({
                "type": "resource_link",
                "uri": format!("file://{name}"),
                "name": name,
            }));
        } else if !content.trim().is_empty() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": format!("[Attached Context File: {name}]\n```\n{content}\n```\n"),
            }));
        }
    }

    let has_screen = attached_files
        .iter()
        .any(|(name, _)| crate::capture::is_image_path(name));
    let mut text = if has_screen {
        prompt.replace("@screen", "[attached screenshot]")
    } else {
        prompt.to_string()
    };
    if has_screen {
        let dims = attached_files
            .iter()
            .filter(|(n, _)| crate::capture::is_image_path(n))
            .find_map(|(n, _)| crate::capture::png_dimensions(Path::new(n)))
            .map(|(w, h)| format!(" Screenshot dimensions: {w}x{h}px."))
            .unwrap_or_default();
        text.push_str(crate::backend::SCREEN_MARKUP_HINT);
        text.push_str(&dims);
    }
    blocks.push(serde_json::json!({ "type": "text", "text": text }));
    blocks
}

/// Base64 for an image attachment, accepting raw file paths or inline data.
fn image_base64(path: &str, content: &str) -> String {
    if let Some(idx) = content.find(";base64,") {
        return content[idx + 8..].to_string();
    }
    if let Ok(bytes) = std::fs::read(path) {
        return crate::capture::base64_encode(&bytes);
    }
    // Caller supplied pre-encoded base64 with no matching file on disk.
    if !content.is_empty() && !std::path::Path::new(path).exists() {
        return content.to_string();
    }
    String::new()
}

/// Converts a `session/update` notification into a `StreamEvent`.
pub fn session_update_to_event(params: &serde_json::Value) -> Option<StreamEvent> {
    let update = params.get("update")?;
    let kind = update
        .get("sessionUpdate")
        .and_then(|k| k.as_str())
        .unwrap_or("");

    match kind {
        "agent_message_chunk" | "text_delta" => {
            let text = extract_chunk_text(update)?;
            Some(StreamEvent::TextDelta(text))
        }
        "agent_thought_chunk" | "thinking_delta" => {
            let text = extract_chunk_text(update)?;
            Some(StreamEvent::ThinkingDelta(text))
        }
        "tool_call" => {
            let id = tool_call_id(update);
            let name = update
                .get("title")
                .or_else(|| update.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string();
            let args = update
                .get("rawInput")
                .or_else(|| update.get("args"))
                .map(|a| a.to_string())
                .unwrap_or_default();
            Some(StreamEvent::ToolStart { id, name, args })
        }
        "tool_call_update" => {
            let status = update.get("status").and_then(|s| s.as_str()).unwrap_or("");
            // Only a terminal state closes the card; in_progress updates are noise.
            if !matches!(status, "completed" | "failed" | "cancelled") {
                return None;
            }
            let id = tool_call_id(update);
            let name = update
                .get("title")
                .or_else(|| update.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string();
            let result = extract_content_text(
                update
                    .get("rawOutput")
                    .or_else(|| update.get("content"))
                    .unwrap_or(&serde_json::Value::Null),
            );
            Some(StreamEvent::ToolEnd {
                id,
                name,
                result,
                is_error: status == "failed",
                duration_ms: 0,
            })
        }
        "usage_update" => {
            let used = update.get("used").and_then(|u| u.as_u64()).unwrap_or(0);
            let size = update.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            Some(StreamEvent::Metrics(UsageMetrics {
                input_tokens: used,
                output_tokens: size,
                total_tokens: used.max(size),
                duration_ms: 0,
            }))
        }
        "available_commands_update" => {
            let commands = update
                .get("availableCommands")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| {
                            let name = c.get("name").and_then(|n| n.as_str())?;
                            let desc = c
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some((name.to_string(), desc))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(StreamEvent::SessionConfig(SessionConfig {
                commands,
                ..Default::default()
            }))
        }
        "config_option_update" | "config_options_update" => {
            let cfg = parse_session_config(update);
            Some(StreamEvent::SessionConfig(cfg))
        }
        "current_mode_update" => {
            let current_mode = update
                .get("currentModeId")
                .or_else(|| update.get("modeId"))
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            if current_mode.is_empty() {
                None
            } else {
                Some(StreamEvent::SessionConfig(SessionConfig {
                    current_mode,
                    ..Default::default()
                }))
            }
        }
        "session_info_update" => {
            let title = update
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !title.is_empty() {
                Some(StreamEvent::SessionTitle(title))
            } else {
                update
                    .get("_meta")
                    .and_then(|m| m.get("opencode/retry"))
                    .and_then(|r| r.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|msg| StreamEvent::ThinkingDelta(format!("Retrying ({msg})…\n")))
            }
        }
        _ => None,
    }
}

fn extract_chunk_text(update: &serde_json::Value) -> Option<String> {
    if let Some(s) = update
        .get("content")
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(arr) = update.get("content").and_then(|c| c.as_array()) {
        let joined: String = arr
            .iter()
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect();
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    update
        .get("text")
        .and_then(|t| t.as_str())
        .map(ToString::to_string)
}

fn tool_call_id(update: &serde_json::Value) -> String {
    update
        .get("toolCallId")
        .and_then(|i| i.as_str())
        .unwrap_or("tool_call")
        .to_string()
}

/// Flattens any tool-output shape into displayable text.
pub fn extract_content_text(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(arr) = value.get("content").and_then(|c| c.as_array()) {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|t| t.as_str())
                    .map(|t| t.to_string())
                    .or_else(|| item.as_str().map(|s| s.to_string()))
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    if let Some(arr) = value.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|t| t.as_str())
                    .map(|t| t.to_string())
                    .or_else(|| {
                        item.get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_string())
                    })
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    if value.is_null() {
        return String::new();
    }
    value.to_string()
}

/// Turns a `session/request_permission` request into a UI-facing request.
pub fn parse_permission_request(
    id: u64,
    params: &serde_json::Value,
) -> PermissionRequest {
    let tool_call = params.get("toolCall");
    let title = tool_call
        .and_then(|t| t.get("title"))
        .and_then(|t| t.as_str())
        .or_else(|| params.get("title").and_then(|t| t.as_str()))
        .unwrap_or("Tool permission")
        .to_string();

    let detail = tool_call
        .and_then(|t| t.get("rawInput").or_else(|| t.get("args")))
        .map(|a| a.to_string())
        .unwrap_or_default();

    let options = params
        .get("options")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    let id = o
                        .get("optionId")
                        .or_else(|| o.get("id"))
                        .and_then(|v| v.as_str())?;
                    let label = o
                        .get("name")
                        .or_else(|| o.get("label"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(id);
                    Some((id.to_string(), label.to_string()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // A well-behaved agent always offers options; supply the standard three
    // when it does not, so the UI never shows an empty decision card.
    let options = if options.is_empty() {
        vec![
            ("allow_once".to_string(), "Allow".to_string()),
            ("allow_always".to_string(), "Allow Always".to_string()),
            ("reject_once".to_string(), "Deny".to_string()),
        ]
    } else {
        options
    };

    PermissionRequest {
        request_id: id,
        title,
        detail,
        options,
    }
}

/// `session/request_permission` reply payload.
pub fn permission_response(option_id: &str) -> serde_json::Value {
    serde_json::json!({
        "outcome": { "outcome": "selected", "optionId": option_id }
    })
}

/// Normalises an option id to one the agent accepts.
pub fn normalize_option_id(option_id: &str) -> &str {
    match option_id {
        "allow" | "allow_once" => "allow_once",
        "allow_always" | "always" => "allow_always",
        "deny" | "deny_once" | "reject" | "reject_once" => "reject_once",
        other => other,
    }
}

/// Chooses the option to auto-approve with, when permissions are granted.
pub fn auto_approve_option(request: &PermissionRequest) -> String {
    request
        .options
        .iter()
        .find(|(id, _)| id == "allow_once" || id == "allow")
        .or_else(|| request.options.iter().find(|(id, _)| id == "allow_always"))
        .or_else(|| request.options.first())
        .map(|(id, _)| normalize_option_id(id).to_string())
        .unwrap_or_else(|| "allow_once".to_string())
}

type RpcWaiterMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, String>>>>>;
type EventTarget = Arc<std::sync::Mutex<Option<(u64, mpsc::UnboundedSender<(u64, StreamEvent)>)>>>;

/// Long-lived concurrent ACP client (`agy acp` / `opencode acp`) using JSON-RPC 2.0 over stdio.
pub struct AcpClient {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_rpc: RpcWaiterMap,
    pending_permissions: Arc<Mutex<HashMap<u64, PermissionRequest>>>,
    event_target: EventTarget,
    auto_approve: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    reader_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
    stderr_tail: Arc<std::sync::Mutex<Vec<String>>>,
    /// session id from `session/new`
    pub session_id: String,
    /// catalogs from `session/new`
    pub config: SessionConfig,
    #[allow(dead_code)]
    workspace: PathBuf,
}

#[allow(dead_code)]
pub type AgyAcpClient = AcpClient;

impl AcpClient {
    /// Spawns the agent, starts concurrent stdout/stderr pumps, and performs `initialize` + `session/new`.
    pub async fn spawn(
        command: &str,
        args: &[String],
        workspace: &Path,
        auto_approve: bool,
        gen: u64,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<Self, String> {
        let bin = resolve_acp_binary(command);
        let mut cmd = Command::new(&bin);
        cmd.args(args)
            .current_dir(workspace)
            .env("PATH", crate::backend::augmented_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn {:?}: {e}", bin))?;
        let stdin = child.stdin.take().ok_or("acp: no stdin")?;
        let stdout = child.stdout.take().ok_or("acp: no stdout")?;
        let stderr = child.stderr.take().ok_or("acp: no stderr")?;

        let stdin = Arc::new(Mutex::new(stdin));
        let pending_rpc: RpcWaiterMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_permissions = Arc::new(Mutex::new(HashMap::new()));
        let event_target: EventTarget = Arc::new(std::sync::Mutex::new(Some((gen, tx))));
        let auto_approve_flag = Arc::new(AtomicBool::new(auto_approve));
        let alive = Arc::new(AtomicBool::new(true));
        let stderr_tail = Arc::new(std::sync::Mutex::new(Vec::new()));

        let stderr_task = {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut err_lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = err_lines.next_line().await {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Ok(mut buf) = tail.lock() {
                        if buf.len() >= 12 {
                            buf.remove(0);
                        }
                        buf.push(trimmed);
                    }
                }
            })
        };

        let reader_task = tokio::spawn(run_stdout_pump(
            stdout,
            stdin.clone(),
            pending_rpc.clone(),
            pending_permissions.clone(),
            event_target.clone(),
            auto_approve_flag.clone(),
            alive.clone(),
            stderr_tail.clone(),
            workspace.to_path_buf(),
        ));

        // initialize
        call_rpc_raw(
            &stdin,
            &pending_rpc,
            &alive,
            &stderr_tail,
            "initialize",
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {
                    "fs": { "readTextFile": true, "writeTextFile": true }
                },
                "clientInfo": { "name": "pi-overlay", "version": env!("CARGO_PKG_VERSION") }
            }),
        )
        .await?;

        // session/new
        let result = call_rpc_raw(
            &stdin,
            &pending_rpc,
            &alive,
            &stderr_tail,
            "session/new",
            serde_json::json!({
                "cwd": workspace.to_string_lossy(),
                "mcpServers": []
            }),
        )
        .await?;

        let session_id = result
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("session")
            .to_string();
        let config = parse_session_config(&result);

        Ok(Self {
            child: Mutex::new(child),
            stdin,
            pending_rpc,
            pending_permissions,
            event_target,
            auto_approve: auto_approve_flag,
            alive,
            reader_task,
            stderr_task,
            stderr_tail,
            session_id,
            config,
            workspace: workspace.to_path_buf(),
        })
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    pub fn set_auto_approve(&self, on: bool) {
        self.auto_approve.store(on, Ordering::Relaxed);
    }

    async fn call_rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        call_rpc_raw(
            &self.stdin,
            &self.pending_rpc,
            &self.alive,
            &self.stderr_tail,
            method,
            params,
        )
        .await
    }

    /// Sends `session/prompt` and awaits turn completion; stdout notifications and
    /// permission requests execute concurrently in the reader pump.
    pub async fn prompt(
        &self,
        blocks: Vec<serde_json::Value>,
        gen: u64,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<(), String> {
        if let Ok(mut guard) = self.event_target.lock() {
            *guard = Some((gen, tx.clone()));
        }

        let result = self
            .call_rpc(
                "session/prompt",
                serde_json::json!({ "sessionId": self.session_id, "prompt": blocks }),
            )
            .await?;

        if let Some(usage) = result.get("usage") {
            let input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output = usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let total = usage
                .get("totalTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(input + output);
            let _ = tx.send((
                gen,
                StreamEvent::Metrics(UsageMetrics {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: total,
                    duration_ms: 0,
                }),
            ));
        }
        Ok(())
    }

    /// Sends `session/set_mode` and waits for server acknowledgement.
    pub async fn set_mode(&self, mode_id: &str) -> Result<(), String> {
        self.call_rpc(
            "session/set_mode",
            serde_json::json!({ "sessionId": self.session_id, "modeId": mode_id }),
        )
        .await
        .map(|_| ())
    }

    /// Sends `session/set_config_option` (`model` or `thought_level`) and awaits acknowledgement.
    pub async fn set_config_option(&self, config_id: &str, value: &str) -> Result<(), String> {
        self.call_rpc(
            "session/set_config_option",
            serde_json::json!({
                "sessionId": self.session_id,
                "configId": config_id,
                "value": value
            }),
        )
        .await
        .map(|_| ())
    }

    /// Sends `session/cancel` for the in-flight prompt.
    pub async fn cancel(&self) -> Result<(), String> {
        write_json_line(
            &self.stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "session/cancel",
                "params": { "sessionId": self.session_id }
            }),
        )
        .await
    }

    /// Answers a pending permission request cleanly while the turn is running.
    pub async fn respond_permission(&self, request_id: u64, option_id: &str) -> Result<(), String> {
        self.pending_permissions.lock().await.remove(&request_id);
        let normalized = normalize_option_id(option_id);
        if normalized == "allow_always" {
            self.auto_approve.store(true, Ordering::Relaxed);
        }
        write_json_line(
            &self.stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": permission_response(normalized)
            }),
        )
        .await
    }

    /// Kills the agent process and aborts background pumps.
    pub async fn shutdown(&self) {
        self.alive.store(false, Ordering::Relaxed);
        self.reader_task.abort();
        self.stderr_task.abort();
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

fn next_id() -> u64 {
    REQUEST_ID_GEN.fetch_add(1, Ordering::Relaxed)
}

async fn write_json_line(
    stdin: &Arc<Mutex<ChildStdin>>,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let mut line = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut guard = stdin.lock().await;
    guard
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("ACP write failed: {e}"))?;
    guard
        .flush()
        .await
        .map_err(|e| format!("ACP flush failed: {e}"))?;
    Ok(())
}

async fn call_rpc_raw(
    stdin: &Arc<Mutex<ChildStdin>>,
    pending_rpc: &RpcWaiterMap,
    alive: &Arc<AtomicBool>,
    stderr_tail: &Arc<std::sync::Mutex<Vec<String>>>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if !alive.load(Ordering::Relaxed) {
        let hint = format_stderr_hint(stderr_tail);
        return Err(format!("ACP agent is not running{hint}"));
    }
    let id = next_id();
    let (tx, rx) = oneshot::channel();
    pending_rpc.lock().await.insert(id, tx);

    if let Err(e) = write_json_line(
        stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }),
    )
    .await
    {
        pending_rpc.lock().await.remove(&id);
        return Err(e);
    }

    rx.await.unwrap_or_else(|_| {
        let hint = format_stderr_hint(stderr_tail);
        Err(format!("ACP channel closed during {method}{hint}"))
    })
}

fn format_stderr_hint(stderr_tail: &Arc<std::sync::Mutex<Vec<String>>>) -> String {
    let Ok(buf) = stderr_tail.lock() else {
        return String::new();
    };
    if buf.is_empty() {
        String::new()
    } else {
        format!(" ({})", buf.join(" | "))
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_stdout_pump(
    stdout: tokio::process::ChildStdout,
    stdin: Arc<Mutex<ChildStdin>>,
    pending_rpc: RpcWaiterMap,
    pending_permissions: Arc<Mutex<HashMap<u64, PermissionRequest>>>,
    event_target: EventTarget,
    auto_approve: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    stderr_tail: Arc<std::sync::Mutex<Vec<String>>>,
    workspace: PathBuf,
) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some(msg) = classify_message(&line) else {
            continue;
        };
        match msg {
            AcpMessage::Response { id, result } => {
                if let Some(waiter) = pending_rpc.lock().await.remove(&id) {
                    let _ = waiter.send(Ok(result));
                }
            }
            AcpMessage::ErrorResponse { id, message } => {
                if let Some(waiter) = pending_rpc.lock().await.remove(&id) {
                    let _ = waiter.send(Err(message));
                }
            }
            AcpMessage::Notification { params, .. } => {
                if let Some(ev) = session_update_to_event(&params) {
                    if let Ok(guard) = event_target.lock() {
                        if let Some((gen, ref tx)) = *guard {
                            let _ = tx.send((gen, ev));
                        }
                    }
                }
            }
            AcpMessage::Request { id, method, params } => {
                match method.as_str() {
                    "session/request_permission" => {
                        let request = parse_permission_request(id, &params);
                        if auto_approve.load(Ordering::Relaxed) {
                            let option = auto_approve_option(&request);
                            let _ = write_json_line(
                                &stdin,
                                &serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": permission_response(&option)
                                }),
                            )
                            .await;
                        } else {
                            pending_permissions.lock().await.insert(id, request.clone());
                            if let Ok(guard) = event_target.lock() {
                                if let Some((gen, ref tx)) = *guard {
                                    let _ = tx.send((gen, StreamEvent::PermissionRequest(request)));
                                }
                            }
                        }
                    }
                    "fs/read_text_file" => {
                        let reply = match handle_fs_read(&params, &workspace) {
                            Ok(content) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": { "content": content }
                            }),
                            Err(msg) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": -32000, "message": msg }
                            }),
                        };
                        let _ = write_json_line(&stdin, &reply).await;
                    }
                    "fs/write_text_file" => {
                        let reply = if !auto_approve.load(Ordering::Relaxed) {
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": -32000, "message": "Write requires Auto mode" }
                            })
                        } else {
                            match handle_fs_write(&params, &workspace) {
                                Ok(()) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {}
                                }),
                                Err(msg) => serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": { "code": -32000, "message": msg }
                                }),
                            }
                        };
                        let _ = write_json_line(&stdin, &reply).await;
                    }
                    _ => {
                        let _ = write_json_line(
                            &stdin,
                            &serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": -32601, "message": format!("Unsupported client method: {method}") }
                            }),
                        )
                        .await;
                    }
                }
            }
        }
    }

    alive.store(false, Ordering::Relaxed);
    let hint = format_stderr_hint(&stderr_tail);
    let mut waiters = pending_rpc.lock().await;
    for (_, waiter) in waiters.drain() {
        let _ = waiter.send(Err(format!("ACP agent closed stdout{hint}")));
    }
}

fn handle_fs_read(params: &serde_json::Value, workspace: &Path) -> Result<String, String> {
    let raw_path = params
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or_else(|| "missing path".to_string())?;
    let candidate = Path::new(raw_path);
    let full = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace.join(candidate)
    };
    let text = std::fs::read_to_string(&full)
        .map_err(|e| format!("failed to read {}: {e}", full.display()))?;

    let start_line = params.get("line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
    let limit = params.get("limit").and_then(|v| v.as_u64()).map(|l| l as usize);
    if start_line == 1 && limit.is_none() {
        return Ok(text);
    }
    let slice: Vec<&str> = text
        .lines()
        .skip(start_line - 1)
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    Ok(slice.join("\n"))
}

fn handle_fs_write(params: &serde_json::Value, workspace: &Path) -> Result<(), String> {
    let raw_path = params
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or_else(|| "missing path".to_string())?;
    let content = params
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| "missing content".to_string())?;
    let candidate = Path::new(raw_path);
    let full = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace.join(candidate)
    };
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&full, content).map_err(|e| format!("failed to write {}: {e}", full.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_notification_has_no_id() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{}}}"#;
        match classify_message(line) {
            Some(AcpMessage::Notification { method, .. }) => assert_eq!(method, "session/update"),
            other => panic!("expected notification, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_server_request_has_id_and_method() {
        let line = r#"{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{}}"#;
        match classify_message(line) {
            Some(AcpMessage::Request { id, method, .. }) => {
                assert_eq!(id, 7);
                assert_eq!(method, "session/request_permission");
            }
            other => panic!("expected request, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_response_has_id_only() {
        let line = r#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#;
        match classify_message(line) {
            Some(AcpMessage::Response { id, result }) => {
                assert_eq!(id, 3);
                assert_eq!(result["stopReason"], "end_turn");
            }
            other => panic!("expected response, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_error_response() {
        let line = r#"{"jsonrpc":"2.0","id":9,"error":{"code":-32601,"message":"no such method"}}"#;
        match classify_message(line) {
            Some(AcpMessage::ErrorResponse { id, message }) => {
                assert_eq!(id, 9);
                assert_eq!(message, "no such method");
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_ignores_garbage() {
        assert!(classify_message("").is_none());
        assert!(classify_message("not json").is_none());
        assert!(classify_message("{}").is_none());
    }

    #[test]
    fn test_agent_message_chunk_becomes_text_delta() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "Hello there" }
            }
        });
        assert_eq!(
            session_update_to_event(&params),
            Some(StreamEvent::TextDelta("Hello there".into()))
        );
    }

    #[test]
    fn test_agent_thought_chunk_becomes_thinking_delta() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "agent_thought_chunk",
                "content": { "type": "text", "text": "pondering" }
            }
        });
        assert_eq!(
            session_update_to_event(&params),
            Some(StreamEvent::ThinkingDelta("pondering".into()))
        );
    }

    #[test]
    fn test_tool_call_becomes_tool_start() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "c1",
                "title": "read_file",
                "kind": "read",
                "rawInput": { "path": "src/main.rs" },
                "status": "in_progress"
            }
        });
        match session_update_to_event(&params) {
            Some(StreamEvent::ToolStart { id, name, args }) => {
                assert_eq!(id, "c1");
                assert_eq!(name, "read_file");
                assert!(args.contains("src/main.rs"));
            }
            other => panic!("expected ToolStart, got {other:?}"),
        }
    }

    #[test]
    fn test_tool_call_update_in_progress_is_ignored() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "c1",
                "status": "in_progress"
            }
        });
        assert_eq!(session_update_to_event(&params), None);
    }

    #[test]
    fn test_tool_call_update_completed_becomes_tool_end() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "c1",
                "title": "read_file",
                "status": "completed",
                "rawOutput": { "content": [{ "type": "text", "text": "file body" }] }
            }
        });
        match session_update_to_event(&params) {
            Some(StreamEvent::ToolEnd { id, result, is_error, .. }) => {
                assert_eq!(id, "c1");
                assert_eq!(result, "file body");
                assert!(!is_error);
            }
            other => panic!("expected ToolEnd, got {other:?}"),
        }
    }

    #[test]
    fn test_tool_call_update_failed_marks_error() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "c2",
                "status": "failed",
                "rawOutput": "boom"
            }
        });
        match session_update_to_event(&params) {
            Some(StreamEvent::ToolEnd { is_error, result, .. }) => {
                assert!(is_error);
                assert_eq!(result, "boom");
            }
            other => panic!("expected ToolEnd, got {other:?}"),
        }
    }

    #[test]
    fn test_available_commands_update_carries_commands() {
        let params = serde_json::json!({
            "update": {
                "sessionUpdate": "available_commands_update",
                "availableCommands": [
                    { "name": "compact", "description": "Shrink context" },
                    { "name": "goal", "description": "Set a goal" }
                ]
            }
        });
        match session_update_to_event(&params) {
            Some(StreamEvent::SessionConfig(cfg)) => {
                assert_eq!(cfg.commands.len(), 2);
                assert_eq!(cfg.commands[0].0, "compact");
                assert_eq!(cfg.commands[1].1, "Set a goal");
            }
            other => panic!("expected SessionConfig, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_session_config_from_new_result() {
        let result = serde_json::json!({
            "sessionId": "abc",
            "models": {
                "currentModelId": "gemini-3-pro",
                "availableModels": [
                    { "modelId": "gemini-3-pro", "name": "Gemini 3 Pro" },
                    { "modelId": "claude-sonnet", "name": "Claude Sonnet" }
                ]
            },
            "modes": {
                "currentModeId": "default",
                "availableModes": [
                    { "id": "default", "name": "Default" },
                    { "id": "auto", "name": "Auto" },
                    { "id": "plan", "name": "Plan" }
                ]
            },
            "configOptions": [
                {
                    "id": "thought_level",
                    "name": "Thinking",
                    "options": [
                        { "value": "low", "name": "Low" },
                        { "value": "high", "name": "High" }
                    ]
                }
            ]
        });
        let cfg = parse_session_config(&result);
        assert_eq!(cfg.current_model, "gemini-3-pro");
        assert_eq!(cfg.models.len(), 2);
        assert_eq!(cfg.models[1].1, "Claude Sonnet");
        assert_eq!(cfg.current_mode, "default");
        assert_eq!(cfg.modes.len(), 3);
        assert_eq!(cfg.thinking_levels, vec!["low", "high"]);
    }

    #[test]
    fn test_parse_session_config_falls_back_to_config_options() {
        let result = serde_json::json!({
            "configOptions": [
                {
                    "id": "model",
                    "name": "Model",
                    "currentValue": "m2",
                    "options": [
                        { "value": "m1", "name": "One" },
                        { "value": "m2", "name": "Two" }
                    ]
                }
            ]
        });
        let cfg = parse_session_config(&result);
        assert_eq!(cfg.models.len(), 2);
        assert_eq!(cfg.current_model, "m2");
    }

    #[test]
    fn test_parse_permission_request_reads_tool_and_options() {
        let params = serde_json::json!({
            "sessionId": "s1",
            "toolCall": {
                "toolCallId": "c9",
                "title": "run_command",
                "rawInput": { "command": "rm -rf build" }
            },
            "options": [
                { "optionId": "allow_once", "name": "Allow" },
                { "optionId": "allow_always", "name": "Allow Always" },
                { "optionId": "reject_once", "name": "Deny" }
            ]
        });
        let req = parse_permission_request(42, &params);
        assert_eq!(req.request_id, 42);
        assert_eq!(req.title, "run_command");
        assert!(req.detail.contains("rm -rf build"));
        assert_eq!(req.options.len(), 3);
        assert_eq!(req.options[0].0, "allow_once");
    }

    #[test]
    fn test_parse_permission_request_supplies_default_options() {
        let params = serde_json::json!({ "toolCall": { "title": "write" } });
        let req = parse_permission_request(1, &params);
        assert_eq!(req.options.len(), 3);
        assert_eq!(req.options[2].0, "reject_once");
    }

    #[test]
    fn test_auto_approve_prefers_allow_once() {
        let options = vec![
            ("reject_once".to_string(), "Deny".to_string()),
            ("allow_always".to_string(), "Allow Always".to_string()),
            ("allow_once".to_string(), "Allow".to_string()),
        ];
        let req = PermissionRequest {
            request_id: 1,
            title: "t".into(),
            detail: String::new(),
            options,
        };
        assert_eq!(auto_approve_option(&req), "allow_once");
    }

    #[test]
    fn test_auto_approve_falls_back_to_allow_always() {
        let req = PermissionRequest {
            request_id: 1,
            title: "t".into(),
            detail: String::new(),
            options: vec![("allow_always".to_string(), "Always".to_string())],
        };
        assert_eq!(auto_approve_option(&req), "allow_always");
    }

    #[test]
    fn test_normalize_option_id_maps_labels() {
        assert_eq!(normalize_option_id("allow"), "allow_once");
        assert_eq!(normalize_option_id("always"), "allow_always");
        assert_eq!(normalize_option_id("deny"), "reject_once");
        assert_eq!(normalize_option_id("allow_once"), "allow_once");
    }

    #[test]
    fn test_permission_response_shape() {
        let v = permission_response("allow_once");
        assert_eq!(v["outcome"]["outcome"], "selected");
        assert_eq!(v["outcome"]["optionId"], "allow_once");
    }

    #[test]
    fn test_build_prompt_blocks_links_files_and_inlines_text() {
        let temp = std::env::temp_dir().join("pi-overlay-acp-test");
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("sample.txt");
        std::fs::write(&path, "x").unwrap();

        let attached = vec![
            (path.to_string_lossy().to_string(), String::new()),
            ("pasted".to_string(), "inline body".to_string()),
        ];
        let blocks = build_prompt_blocks("Explain", &attached);
        assert_eq!(blocks.last().unwrap()["text"], "Explain");
        assert!(blocks
            .iter()
            .any(|b| b["type"] == "resource_link" && b["uri"].as_str().unwrap().starts_with("file://")));
        assert!(blocks.iter().any(|b| b["type"] == "text"
            && b["text"].as_str().unwrap().contains("inline body")));
    }

    #[test]
    fn test_build_prompt_blocks_image_and_markup_hint() {
        let temp = std::env::temp_dir().join("pi-overlay-acp-img");
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("shot.png");
        std::fs::write(&path, [0x89u8, 0x50, 0x4E, 0x47]).unwrap();

        let attached = vec![(path.to_string_lossy().to_string(), String::new())];
        let blocks = build_prompt_blocks("What is this?", &attached);
        assert!(blocks.iter().any(|b| b["type"] == "image"));
        let image = blocks.iter().find(|b| b["type"] == "image").unwrap();
        assert_eq!(image["mimeType"], "image/png");
        assert!(!image["data"].as_str().unwrap().is_empty());
        let text = blocks.last().unwrap()["text"].as_str().unwrap();
        assert!(text.contains("[[markup: box"));
        assert!(text.contains("What is this?"));
    }

    #[test]
    fn test_resolve_acp_binary_uses_configured_path() {
        let resolved = resolve_acp_binary("/nonexistent/agent");
        assert_eq!(resolved, PathBuf::from("/nonexistent/agent"));
    }

    #[test]
    fn test_resolve_acp_binary_defaults_to_agy() {
        assert_eq!(resolve_acp_binary(""), PathBuf::from("agy"));
        assert_eq!(resolve_acp_binary("   "), PathBuf::from("agy"));
    }

    #[test]
    fn test_session_info_and_mode_updates() {
        let title_update = serde_json::json!({
            "update": { "sessionUpdate": "session_info_update", "title": "Refactor Plan" }
        });
        assert_eq!(
            session_update_to_event(&title_update),
            Some(StreamEvent::SessionTitle("Refactor Plan".into()))
        );

        let mode_update = serde_json::json!({
            "update": { "sessionUpdate": "current_mode_update", "currentModeId": "plan" }
        });
        match session_update_to_event(&mode_update) {
            Some(StreamEvent::SessionConfig(cfg)) => assert_eq!(cfg.current_mode, "plan"),
            other => panic!("expected SessionConfig, got {other:?}"),
        }
    }
}
