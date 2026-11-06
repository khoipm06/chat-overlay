use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

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

/// ACP Initialize Request & Response
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitializeParams {
    pub protocol_version: u16,
    pub client_info: ClientInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitializeResult {
    pub protocol_version: u16,
    pub agent_info: AgentInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub version: String,
}

/// ACP Session Prompt Request
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PromptParams {
    pub session_id: Option<String>,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
}

/// ACP session/update notification payload
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionUpdateNotification {
    pub session_id: String,
    pub update: SessionUpdatePayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "sessionUpdate")]
pub enum SessionUpdatePayload {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { text: String },
    #[serde(rename = "tool_call")]
    ToolCall {
        name: String,
        args: String,
        #[serde(default)]
        status: Option<String>,
    },
    #[serde(rename = "usage")]
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
        duration_ms: u64,
    },
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "error")]
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_request_serialization() {
        let req = JsonRpcRequest::new(
            "session/prompt",
            Some(PromptParams {
                session_id: Some("sess_123".to_string()),
                prompt: "Hello ACP".to_string(),
                model: None,
                thinking_level: Some("high".to_string()),
            }),
        );

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"session/prompt\""));
        assert!(json.contains("Hello ACP"));
    }

    #[test]
    fn test_acp_update_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "session_id": "sess_42",
                "update": {
                    "sessionUpdate": "thinking_delta",
                    "text": "Tư duy về thuật toán..."
                }
            }
        }"#;

        let notif: JsonRpcNotification<SessionUpdateNotification> =
            serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "session/update");
        match notif.params.unwrap().update {
            SessionUpdatePayload::ThinkingDelta { text } => {
                assert_eq!(text, "Tư duy về thuật toán...");
            }
            _ => panic!("Expected ThinkingDelta"),
        }
    }
}

use crate::backend::{StreamEvent, UsageMetrics};

/// Parses an incoming JSON line as an ACP session/update notification, if valid.
pub fn parse_acp_message(json_str: &str) -> Option<StreamEvent> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(notif) = serde_json::from_str::<JsonRpcNotification<SessionUpdateNotification>>(trimmed) {
        if notif.method == "session/update" {
            if let Some(params) = notif.params {
                match params.update {
                    SessionUpdatePayload::TextDelta { text } => {
                        return Some(StreamEvent::TextDelta(text));
                    }
                    SessionUpdatePayload::ThinkingDelta { text } => {
                        return Some(StreamEvent::ThinkingDelta(text));
                    }
                    SessionUpdatePayload::ToolCall { name, args, status } => {
                        if status.as_deref() == Some("done") {
                            return Some(StreamEvent::ToolEnd {
                                id: "acp_tool".to_string(),
                                name,
                                result: String::new(),
                                is_error: false,
                                duration_ms: 0,
                            });
                        }
                        return Some(StreamEvent::ToolStart {
                            id: "acp_tool".to_string(),
                            name,
                            args,
                        });
                    }
                    SessionUpdatePayload::Usage {
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        duration_ms,
                    } => {
                        return Some(StreamEvent::Metrics(UsageMetrics {
                            input_tokens,
                            output_tokens,
                            total_tokens,
                            duration_ms,
                        }));
                    }
                    SessionUpdatePayload::Done => {
                        return Some(StreamEvent::Done);
                    }
                    SessionUpdatePayload::Error { message } => {
                        return Some(StreamEvent::Error(message));
                    }
                }
            }
        }
    }
    None
}

#[test]
fn test_parse_acp_to_stream_event() {
    let json = r#"{
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "session_id": "test_1",
            "update": {
                "sessionUpdate": "text_delta",
                "text": "Hello ACP World!"
            }
        }
    }"#;

    let ev = parse_acp_message(json).expect("Must parse ACP TextDelta");
    assert_eq!(ev, StreamEvent::TextDelta("Hello ACP World!".to_string()));
}
