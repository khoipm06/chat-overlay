use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// Incremental text delta for live typing output
    TextDelta(String),
    /// Thinking indicator updates
    ThinkingDelta(String),
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
            .arg(session_dir)
            .arg("--")
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
/// Falls back to raw text if not JSON.
fn parse_and_dispatch_line(
    raw_line: &str,
    gen: u64,
    tx: &mpsc::UnboundedSender<(u64, StreamEvent)>,
) -> Result<(), ()> {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
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
                                if let Some(delta) = sub_event.get("delta").and_then(|d| d.as_str()) {
                                    if tx.send((gen, StreamEvent::TextDelta(delta.to_string()))).is_err() {
                                        return Err(());
                                    }
                                }
                            }
                            "thinking_delta" => {
                                if let Some(delta) = sub_event.get("delta").and_then(|d| d.as_str()) {
                                    if tx.send((gen, StreamEvent::ThinkingDelta(delta.to_string()))).is_err() {
                                        return Err(());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "agent_end" | "agent_settled" => {
                    let _ = tx.send((gen, StreamEvent::Done));
                }
                _ => {}
            }
            return Ok(());
        }
    }

    // Fallback: If not standard json event, emit as plain line
    if tx.send((gen, StreamEvent::Content(raw_line.to_string()))).is_err() {
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
        assert_eq!(thinking_delta, StreamEvent::ThinkingDelta("thinking...".into()));
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

        // 3. Settled
        let json_settled = r#"{"type":"agent_settled","aborted":false}"#;
        assert!(parse_and_dispatch_line(json_settled, gen, &tx).is_ok());

        // 4. Plain text fallback
        let plain = "Non-json line";
        assert!(parse_and_dispatch_line(plain, gen, &tx).is_ok());

        let ev1 = rx.try_recv().unwrap();
        assert_eq!(ev1, (10, StreamEvent::TextDelta("Hello ".to_string())));

        let ev2 = rx.try_recv().unwrap();
        assert_eq!(ev2, (10, StreamEvent::ThinkingDelta("pondering".to_string())));

        let ev3 = rx.try_recv().unwrap();
        assert_eq!(ev3, (10, StreamEvent::Done));

        let ev4 = rx.try_recv().unwrap();
        assert_eq!(ev4, (10, StreamEvent::Content("Non-json line".to_string())));
    }
}
