use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

pub fn get_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join("pi-ephemeral-overlay.sock")
}

pub fn try_send_toggle() -> bool {
    use std::io::Write;
    use std::os::unix::net::UnixStream as StdUnixStream;

    let socket_path = get_socket_path();
    if let Ok(mut stream) = StdUnixStream::connect(&socket_path) {
        let _ = stream.write_all(b"TOGGLE\n");
        let _ = stream.flush();
        true
    } else {
        // Stale socket or not running: remove if broken file exists
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        false
    }
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
                eprintln!(
                    "[pi-overlay] Failed to bind IPC socket at {:?}: {}",
                    socket_path, e
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_structure() {
        let path = get_socket_path();
        assert!(path.ends_with("pi-ephemeral-overlay.sock"));
    }
}
