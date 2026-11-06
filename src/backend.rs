use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub struct UsageMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    /// Incremental text delta for live typing output
    TextDelta(String),
    /// Thinking indicator updates
    ThinkingDelta(String),
    /// Tool execution notification (e.g. "Running bash: whoami")
    ToolCall { name: String, args: String },
    /// Token usage and latency metrics from model
    Metrics(UsageMetrics),
    /// Backward-compatible full line
    Content(String),
    /// Error message output
    Error(String),
    /// Final completion of assistant turn
    Done,
}

pub struct PiRunner {
    child: Option<Child>,
    generation: u64,
}

impl PiRunner {
    pub fn new() -> Self {
        Self {
            child: None,
            generation: 1,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Immediately abort active task: increment generation synchronously to cancel stale events,
    /// and asynchronously reap child process to prevent zombie processes.
    pub fn abort(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(mut child) = self.child.take() {
            tokio::spawn(async move {
                let _ = child.kill().await;
                let _ = child.wait().await;
            });
        }
    }

    pub fn spawn_prompt(
        &mut self,
        prompt: String,
        model: Option<String>,
        thinking_level: Option<String>,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Abort previous session before launching new prompt
        self.abort();
        let gen = self.generation;

        // Ephemeral temp session directory
        let session_dir = std::env::temp_dir().join("pi-overlay-sessions");
        let _ = std::fs::create_dir_all(&session_dir);

        let mut cmd = Command::new("pi");
        cmd.arg("-p")
            .arg("--mode")
            .arg("json")
            .arg("--session-dir")
            .arg(session_dir);

        if let Some(m) = model {
            if m != "default" && !m.trim().is_empty() {
                cmd.arg("--model").arg(m);
            }
        }

        if let Some(th) = thinking_level {
            if !th.trim().is_empty() {
                cmd.arg("--thinking").arg(th);
            }
        }

        cmd.arg("--")
            .arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // Task reading stdout stream: parse JSONL live events or fallback to lines
        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if parse_and_dispatch_line(&line, gen, &tx_out).is_err() {
                    return;
                }
            }
            let _ = tx_out.send((gen, StreamEvent::Done));
        });

        // Task reading stderr stream (avoids pipe deadlock and forwards errors)
        let tx_err = tx;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = tx_err.send((gen, StreamEvent::Error(line)));
                }
            }
        });

        self.child = Some(child);
        Ok(())
    }
}

/// Parse one JSONL line from pi --mode json and emit appropriate StreamEvent.
fn parse_and_dispatch_line(
    raw_line: &str,
    gen: u64,
    tx: &mpsc::UnboundedSender<(u64, StreamEvent)>,
) -> Result<(), ()> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    // 1. Check for standard ACP (Agent Client Protocol) JSON-RPC notifications
    if let Some(acp_event) = crate::acp::parse_acp_message(trimmed) {
        if tx.send((gen, acp_event)).is_err() {
            return Err(());
        }
        return Ok(());
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(event_type) = v.get("type").and_then(|t| t.as_str()) {
            match event_type {
                "message_update" => {
                    if let Some(sub_event) = v.get("assistantMessageEvent") {
                        let sub_type = sub_event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match sub_type {
                            "text_delta" => {
                                if let Some(delta) = sub_event.get("delta").and_then(|d| d.as_str())
                                {
                                    if tx
                                        .send((gen, StreamEvent::TextDelta(delta.to_string())))
                                        .is_err()
                                    {
                                        return Err(());
                                    }
                                }
                            }
                            "thinking_delta" => {
                                if let Some(delta) = sub_event.get("delta").and_then(|d| d.as_str())
                                {
                                    if tx
                                        .send((gen, StreamEvent::ThinkingDelta(delta.to_string())))
                                        .is_err()
                                    {
                                        return Err(());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "tool_execution_start" => {
                    let name = v.get("toolName").and_then(|n| n.as_str()).unwrap_or("tool");
                    let args = v.get("args").map(|a| a.to_string()).unwrap_or_default();
                    let _ = tx.send((
                        gen,
                        StreamEvent::ToolCall {
                            name: name.to_string(),
                            args,
                        },
                    ));
                }
                "message_end" => {
                    if let Some(msg) = v.get("message") {
                        if let Some(usage) = msg.get("usage") {
                            let input_tokens =
                                usage.get("input").and_then(|n| n.as_u64()).unwrap_or(0);
                            let output_tokens =
                                usage.get("output").and_then(|n| n.as_u64()).unwrap_or(0);
                            let total_tokens = usage
                                .get("totalTokens")
                                .and_then(|n| n.as_u64())
                                .unwrap_or(0);
                            let duration_ms =
                                msg.get("durationMs").and_then(|n| n.as_u64()).unwrap_or(0);
                            if total_tokens > 0 {
                                let _ = tx.send((
                                    gen,
                                    StreamEvent::Metrics(UsageMetrics {
                                        input_tokens,
                                        output_tokens,
                                        total_tokens,
                                        duration_ms,
                                    }),
                                ));
                            }
                        }
                    }
                }
                "agent_end" | "agent_settled" => {
                    let _ = tx.send((gen, StreamEvent::Done));
                }
                // Ignore other internal protocol events (session, agent_start, turn_start, turn_end, etc.)
                _ => {}
            }
            return Ok(());
        }
    }

    // Fallback: If not standard json event, emit as plain line only if non-empty and non-JSON
    if !trimmed.starts_with('{')
        && tx
            .send((gen, StreamEvent::Content(raw_line.to_string())))
            .is_err()
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_initial_state() {
        let runner = PiRunner::new();
        assert_eq!(runner.generation(), 1);
        assert!(runner.child.is_none());
    }

    #[test]
    fn test_runner_abort_increments_generation() {
        let mut runner = PiRunner::new();
        runner.abort();
        assert_eq!(runner.generation(), 2);
        runner.abort();
        assert_eq!(runner.generation(), 3);
    }

    #[test]
    fn test_stream_event_variants() {
        let content = StreamEvent::Content("hello world".into());
        let text_delta = StreamEvent::TextDelta("delta chunk".into());
        let thinking_delta = StreamEvent::ThinkingDelta("thinking...".into());
        let error = StreamEvent::Error("failed to run".into());
        let done = StreamEvent::Done;

        assert_ne!(content, error);
        assert_eq!(done, StreamEvent::Done);
        assert_eq!(text_delta, StreamEvent::TextDelta("delta chunk".into()));
        assert_eq!(
            thinking_delta,
            StreamEvent::ThinkingDelta("thinking...".into())
        );
    }

    #[test]
    fn test_parse_jsonl_stream_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let gen = 10;

        // 1. Text delta
        let json_text = r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":1,"delta":"Hello "}}"#;
        assert!(parse_and_dispatch_line(json_text, gen, &tx).is_ok());

        // 2. Thinking delta
        let json_think = r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"thinking_delta","contentIndex":0,"delta":"pondering"}}"#;
        assert!(parse_and_dispatch_line(json_think, gen, &tx).is_ok());

        // 3. Tool call
        let json_tool = r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"bash","args":{"command":"ls"}}"#;
        assert!(parse_and_dispatch_line(json_tool, gen, &tx).is_ok());

        // 4. Settled
        let json_settled = r#"{"type":"agent_settled","aborted":false}"#;
        assert!(parse_and_dispatch_line(json_settled, gen, &tx).is_ok());

        let ev1 = rx.try_recv().unwrap();
        assert_eq!(ev1, (10, StreamEvent::TextDelta("Hello ".to_string())));

        let ev2 = rx.try_recv().unwrap();
        assert_eq!(
            ev2,
            (10, StreamEvent::ThinkingDelta("pondering".to_string()))
        );

        let ev3 = rx.try_recv().unwrap();
        assert_eq!(
            ev3,
            (
                10,
                StreamEvent::ToolCall {
                    name: "bash".to_string(),
                    args: r#"{"command":"ls"}"#.to_string()
                }
            )
        );

        let ev4 = rx.try_recv().unwrap();
        assert_eq!(ev4, (10, StreamEvent::Done));
    }

    #[test]
    fn test_parse_jsonl_fuzz_and_edge_cases() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let gen = 42;

        let edge_cases = [
            // Nested complex tool call args with unicode and escaped json
            r#"{"type":"tool_execution_start","toolName":"edit","args":{"nested":{"code":"fn main() {\n    println!(\"Xin chào thế giới 🦀\");\n}","diff":"@@ -1 +1 @@\n-old\n+new"},"list":[1,2,3,{"a":"b"}]}}"#,
            // Tool call with empty args or non-string/object
            r#"{"type":"tool_execution_start","toolName":"bash"}"#,
            // Unicode characters in tool call
            r#"{"type":"tool_execution_start","toolName":"read","args":{"path":"/đường_dẫn/đến/tệp_tin.txt"}}"#,
            // Malformed JSON (must not crash, must not emit as content if starts with '{')
            r#"{"type":"message_update", malformed json syntax"#,
            // Arbitrary protocol events that should be safely ignored
            r#"{"type":"agent_start","model":"claude-3-7-sonnet"}"#,
            r#"{"type":"turn_start","turn":1}"#,
            r#"{"type":"turn_end","turn":1}"#,
            // Message update with missing fields or unknown sub-event
            r#"{"type":"message_update"}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"unknown_future_delta"}}"#,
            // Metrics with partial/zero usage
            r#"{"type":"message_end","message":{"usage":{"input":100,"output":50,"totalTokens":150},"durationMs":1200}}"#,
            // Metrics with empty usage
            r#"{"type":"message_end","message":{}}"#,
        ];

        for line in edge_cases {
            let res = parse_and_dispatch_line(line, gen, &tx);
            assert!(res.is_ok(), "Failed on edge case: {}", line);
        }

        // Verify tool call 1
        let (g1, ev1) = rx.try_recv().unwrap();
        assert_eq!(g1, 42);
        match ev1 {
            StreamEvent::ToolCall { name, args } => {
                assert_eq!(name, "edit");
                assert!(args.contains("Xin chào thế giới 🦀"));
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }

        // Verify tool call 2
        let (g2, ev2) = rx.try_recv().unwrap();
        assert_eq!(g2, 42);
        match ev2 {
            StreamEvent::ToolCall { name, args } => {
                assert_eq!(name, "bash");
                assert_eq!(args, "");
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }

        // Verify tool call 3
        let (g3, ev3) = rx.try_recv().unwrap();
        assert_eq!(g3, 42);
        match ev3 {
            StreamEvent::ToolCall { name, args } => {
                assert_eq!(name, "read");
                assert!(args.contains("tệp_tin.txt"));
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }

        // Verify metrics
        let (g4, ev4) = rx.try_recv().unwrap();
        assert_eq!(g4, 42);
        assert_eq!(
            ev4,
            StreamEvent::Metrics(UsageMetrics {
                input_tokens: 100,
                output_tokens: 50,
                total_tokens: 150,
                duration_ms: 1200,
            })
        );

        // Ensure no stray events leaked
        assert!(rx.try_recv().is_err());
    }
}
