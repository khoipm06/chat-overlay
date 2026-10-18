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

    /// Hủy tác vụ hiện tại ngay lập tức: tăng generation đồng bộ để ngắt các event cũ
    /// và reap child process bất đồng bộ tránh zombie.
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
        // Hủy session cũ trước khi khởi động session mới
        self.abort();
        let gen = self.generation;

        // Thư mục session tạm cho chế độ ephemeral
        let session_dir = std::env::temp_dir().join("pi-overlay-sessions");
        let _ = std::fs::create_dir_all(&session_dir);

        let mut cmd = Command::new("pi");
        cmd.arg("-p")
            .arg(prompt)
            .arg("--session-dir")
            .arg(session_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // Task đọc stdout
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

        // Task đọc stderr (tránh pipe buffer deadlock và chuyển thông báo lỗi)
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
