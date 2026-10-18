use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

pub fn get_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join("pi-ephemeral-overlay.sock")
}

pub async fn send_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = get_socket_path();
    let mut stream = UnixStream::connect(socket_path).await?;
    stream.write_all(b"TOGGLE\n").await?;
    Ok(())
}

pub fn start_ipc_listener(tx: mpsc::UnboundedSender<()>) {
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);

    tokio::spawn(async move {
        match UnixListener::bind(&socket_path) {
            Ok(listener) => {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 32];
                    if let Ok(n) = stream.read(&mut buf).await {
                        if n > 0 {
                            let msg = String::from_utf8_lossy(&buf[..n]);
                            if msg.trim() == "TOGGLE" {
                                let _ = tx.send(());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[pi-overlay] Failed to bind IPC socket at {:?}: {}", socket_path, e);
            }
        }
    });
}
