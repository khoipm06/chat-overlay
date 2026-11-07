use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Instant;

use crate::settings::AppSettings;

static ACTIVE_RECORDER: Mutex<Option<ActiveRecording>> = Mutex::new(None);

struct ActiveRecording {
    child: Child,
    output_path: PathBuf,
    #[allow(dead_code)]
    started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationStatus {
    Idle,
    Recording,
    Transcribing,
}

fn command_exists(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join(cmd).is_file() {
                return true;
            }
        }
    }
    false
}

/// Starts audio recording from the default microphone into an ephemeral WAV file.
pub fn start_recording() -> Result<PathBuf, String> {
    let mut lock = ACTIVE_RECORDER.lock().map_err(|e| e.to_string())?;
    if lock.is_some() {
        return Err("Audio recording is already in progress".to_string());
    }

    let out_dir = std::env::temp_dir();
    let out_file = out_dir.join("pi-overlay-dictation.wav");
    let _ = std::fs::remove_file(&out_file);

    let out_str = out_file.to_string_lossy().to_string();

    let child = if command_exists("pw-record") {
        Command::new("pw-record")
            .arg("--rate")
            .arg("16000")
            .arg("--channels")
            .arg("1")
            .arg(&out_str)
            .spawn()
            .map_err(|e| format!("Failed to start pw-record: {}", e))?
    } else if command_exists("arecord") {
        Command::new("arecord")
            .arg("-r")
            .arg("16000")
            .arg("-c")
            .arg("1")
            .arg("-f")
            .arg("S16_LE")
            .arg(&out_str)
            .spawn()
            .map_err(|e| format!("Failed to start arecord: {}", e))?
    } else {
        return Err("Neither 'pw-record' nor 'arecord' was found for audio capture".to_string());
    };

    *lock = Some(ActiveRecording {
        child,
        output_path: out_file.clone(),
        started_at: Instant::now(),
    });

    Ok(out_file)
}

/// Stops the active recording process and returns the path to the recorded WAV file.
pub fn stop_recording() -> Result<PathBuf, String> {
    let mut rec = {
        let mut lock = ACTIVE_RECORDER.lock().map_err(|e| e.to_string())?;
        lock.take().ok_or("No active audio recording to stop")?
    };

    // Send SIGINT or kill the recorder child process cleanly
    let _ = rec.child.kill();
    let _ = rec.child.wait();

    // Small delay to ensure PipeWire / ALSA finishes flushing header to disk
    std::thread::sleep(std::time::Duration::from_millis(150));

    if !rec.output_path.exists() {
        return Err("Recorded audio file was not created".to_string());
    }

    let metadata = std::fs::metadata(&rec.output_path)
        .map_err(|e| format!("Failed to read recorded audio metadata: {}", e))?;
    if metadata.len() < 100 {
        return Err("Recorded audio file is too short or empty".to_string());
    }

    Ok(rec.output_path)
}

/// Checks if an active recording session is currently running.
pub fn is_recording() -> bool {
    if let Ok(lock) = ACTIVE_RECORDER.lock() {
        lock.is_some()
    } else {
        false
    }
}

/// Transcribes the recorded audio file using the configured backend in settings or auto-detection.
pub fn transcribe_audio(audio_path: &Path, settings: &AppSettings) -> Result<String, String> {
    let audio_str = audio_path.to_string_lossy().to_string();

    // 1. Explicit custom command configured in settings
    if let Some(ref cmd_tmpl) = settings.dictation_command {
        let trimmed = cmd_tmpl.trim();
        if !trimmed.is_empty() {
            let cmd_str = trimmed.replace("{file}", &format!("\"{}\"", audio_str));
            let output = Command::new("sh")
                .arg("-c")
                .arg(&cmd_str)
                .output()
                .map_err(|e| format!("Failed to execute custom dictation command: {}", e))?;
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !text.is_empty() {
                    return Ok(text);
                }
            } else {
                let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(format!("Custom dictation command error: {}", err));
            }
        }
    }

    // 2. HTTP API Endpoint (OpenAI / Groq / Local Whisper API)
    let api_url = settings
        .dictation_api_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .map(|u| u.to_string())
        .or_else(|| {
            if std::env::var("GROQ_API_KEY").is_ok() {
                Some("https://api.groq.com/openai/v1/audio/transcriptions".to_string())
            } else if std::env::var("OPENAI_API_KEY").is_ok() {
                Some("https://api.openai.com/v1/audio/transcriptions".to_string())
            } else {
                None
            }
        });

    if let Some(endpoint) = api_url {
        let api_key = settings
            .dictation_api_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
            .map(|k| k.to_string())
            .or_else(|| std::env::var("GROQ_API_KEY").ok())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();

        let model = settings
            .dictation_model
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| {
                if endpoint.contains("groq.com") {
                    "whisper-large-v3-turbo"
                } else {
                    "whisper-1"
                }
            });

        let mut curl = Command::new("curl");
        curl.arg("-s").arg("-X").arg("POST").arg(&endpoint);

        if !api_key.is_empty() {
            curl.arg("-H").arg(format!("Authorization: Bearer {}", api_key));
        }

        curl.arg("-F").arg(format!("file=@{}", audio_str));
        curl.arg("-F").arg(format!("model={}", model));

        if let Some(ref lang) = settings.dictation_language {
            let l = lang.trim();
            if !l.is_empty() && l != "auto" {
                curl.arg("-F").arg(format!("language={}", l));
            }
        }

        let output = curl
            .output()
            .map_err(|e| format!("Failed to run curl for dictation API: {}", e))?;

        if output.status.success() {
            let res_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&res_str) {
                if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Ok(trimmed.to_string());
                    }
                } else if let Some(err_obj) = v.get("error").and_then(|e| e.get("message")) {
                    return Err(format!("API error: {}", err_obj));
                }
            }
        }
    }

    // 3. System CLI Tools Auto-Detection (whisper-cli, whisper)
    if command_exists("whisper-cli") {
        let mut cmd = Command::new("whisper-cli");
        cmd.arg("-f").arg(&audio_str).arg("--no-timestamps");
        if let Some(ref lang) = settings.dictation_language {
            let l = lang.trim();
            if !l.is_empty() && l != "auto" {
                cmd.arg("-l").arg(l);
            }
        }
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !text.is_empty() {
                    return Ok(text);
                }
            }
        }
    }

    if command_exists("whisper") {
        let out_dir = std::env::temp_dir();
        let mut cmd = Command::new("whisper");
        cmd.arg(&audio_str)
            .arg("--output_format")
            .arg("txt")
            .arg("--output_dir")
            .arg(&out_dir);
        if let Some(ref lang) = settings.dictation_language {
            let l = lang.trim();
            if !l.is_empty() && l != "auto" {
                cmd.arg("--language").arg(l);
            }
        }
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let txt_file = out_dir.join("pi-overlay-dictation.txt");
                if let Ok(content) = std::fs::read_to_string(&txt_file) {
                    let _ = std::fs::remove_file(&txt_file);
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        return Ok(trimmed.to_string());
                    }
                }
            }
        }
    }

    // 4. Clean fallback notification when no STT engine is configured
    Err("Audio captured to /tmp/pi-overlay-dictation.wav. Configure a transcription API URL (e.g. Groq, OpenAI, or 9Router) or whisper command in Settings.".to_string())
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_custom_command_template_replacement() {
        let tmpl = "whisper-cli -f {file} --no-timestamps";
        let path = "/tmp/test.wav";
        let res = tmpl.replace("{file}", &format!("\"{}\"", path));
        assert_eq!(res, "whisper-cli -f \"/tmp/test.wav\" --no-timestamps");
    }

    #[test]
    fn test_is_recording_initially_false() {
        assert!(!super::is_recording());
    }

    #[test]
    fn test_parse_json_transcription_error() {
        let json_body = r#"{"error":{"message":"Invalid API key or quota exceeded"}}"#;
        let v: serde_json::Value = serde_json::from_str(json_body).unwrap();
        let err_msg = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str());
        assert_eq!(err_msg, Some("Invalid API key or quota exceeded"));
    }

    #[test]
    fn test_dictation_status_enum() {
        assert_eq!(super::DictationStatus::Idle, super::DictationStatus::Idle);
        assert_ne!(super::DictationStatus::Recording, super::DictationStatus::Transcribing);
    }
}
