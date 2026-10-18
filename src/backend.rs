use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub enum StreamEvent {
    Content(String),
    Error(String),
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
            .arg("--session-dir")
            .arg(session_dir)
            .arg("--")
            .arg(prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // Task reading stdout stream
        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx_out.send((gen, StreamEvent::Content(line))).is_err() {
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
        let error = StreamEvent::Error("failed to run".into());
        let done = StreamEvent::Done;

        match content {
            StreamEvent::Content(c) => assert_eq!(c, "hello world"),
            _ => panic!("Expected Content variant"),
        }

        match error {
            StreamEvent::Error(e) => assert_eq!(e, "failed to run"),
            _ => panic!("Expected Error variant"),
        }

        matches!(done, StreamEvent::Done);
    }
}
