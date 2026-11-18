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

/// Models / modes / thinking levels / slash commands reported by the harness.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SessionConfig {
    pub models: Vec<(String, String)>,
    pub current_model: String,
    pub modes: Vec<(String, String)>,
    pub current_mode: String,
    pub thinking_levels: Vec<String>,
    pub commands: Vec<(String, String)>,
}

/// A tool permission request the agent is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    /// JSON-RPC id to answer with `respond_permission`.
    pub request_id: u64,
    pub title: String,
    pub detail: String,
    /// `(option_id, label)`, e.g. `("allow_once", "Allow")`.
    pub options: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    /// Incremental text delta for live typing output
    TextDelta(String),
    /// Thinking indicator updates
    ThinkingDelta(String),
    /// Tool execution started: id, name, formatted/raw args
    ToolStart {
        id: String,
        name: String,
        args: String,
    },
    /// Tool execution finished: id, name, output result, is_error, duration_ms
    ToolEnd {
        id: String,
        name: String,
        result: String,
        is_error: bool,
        duration_ms: u64,
    },
    /// Backward-compatible ToolCall
    ToolCall { name: String, args: String },
    /// Token usage and latency metrics from model
    Metrics(UsageMetrics),
    /// Backward-compatible full line
    Content(String),
    /// Model / mode / command catalogs for the composer.
    SessionConfig(SessionConfig),
    /// Tool permission request awaiting a user decision.
    PermissionRequest(PermissionRequest),
    /// Error message output
    Error(String),
    /// Dynamic conversation title reported by ACP `session_info_update`.
    SessionTitle(String),
    /// Final completion of assistant turn
    Done,
}

pub struct PiRunner {
    child: Option<Child>,
    generation: u64,
    /// Session id reused across turns so the agent keeps conversation context.
    session_id: String,
    /// Directory the agent runs in (workspace / `cwd`).
    workspace_dir: std::path::PathBuf,
}

/// The active agent backend.
///
/// Both variants share one interface so the UI never branches on which harness
/// is running: it builds a `Harness`, calls `spawn_prompt`, and drains events.
pub enum Harness {
    Pi(PiRunner),
    Acp(AgyAcpRunner),
}

impl Harness {
    /// Builds the harness named by `kind`, wired to the configured workspace.
    pub fn from_settings(
        kind: crate::settings::HarnessKind,
        settings: &crate::settings::AppSettings,
    ) -> Self {
        let workspace = std::path::PathBuf::from(&settings.workspace_dir);
        match kind {
            crate::settings::HarnessKind::PiJson => {
                let mut runner = PiRunner::new();
                runner.set_workspace_dir(workspace);
                Harness::Pi(runner)
            }
            crate::settings::HarnessKind::AgyAcp => {
                let mut runner = AgyAcpRunner::new(
                    settings.acp_command.clone(),
                    settings.acp_args.clone(),
                    workspace,
                );
                runner.set_auto_approve(
                    settings.auto_approve_permissions || settings.acp_mode == "auto",
                );
                Harness::Acp(runner)
            }
            crate::settings::HarnessKind::OpenCodeAcp => {
                let mut runner = AgyAcpRunner::new(
                    settings.opencode_command.clone(),
                    settings.opencode_args.clone(),
                    workspace,
                );
                runner.set_auto_approve(
                    settings.auto_approve_permissions || settings.acp_mode == "auto",
                );
                Harness::Acp(runner)
            }
        }
    }

    pub fn generation(&self) -> u64 {
        match self {
            Harness::Pi(r) => r.generation(),
            Harness::Acp(r) => r.generation(),
        }
    }

    pub fn abort(&mut self) {
        match self {
            Harness::Pi(r) => r.abort(),
            Harness::Acp(r) => r.abort(),
        }
    }

    /// Eagerly spawns the ACP agent so its model/mode/command catalogs hydrate
    /// before the user submits the first prompt (no-op for Pi JSON).
    pub fn warmup(&mut self, tx: mpsc::UnboundedSender<(u64, StreamEvent)>) {
        if let Harness::Acp(r) = self {
            r.warmup(tx);
        }
    }

    /// Starts a fresh conversation on the current harness.
    pub fn new_session(&mut self) {
        match self {
            Harness::Pi(r) => r.new_session(),
            Harness::Acp(r) => r.new_session(),
        }
    }

    /// Rebinds the working directory. Always starts a new session, because one
    /// conversation is scoped to one workspace.
    pub fn set_workspace_dir(&mut self, dir: impl Into<std::path::PathBuf>) {
        match self {
            Harness::Pi(r) => r.set_workspace_dir(dir),
            Harness::Acp(r) => r.set_workspace_dir(dir),
        }
    }

    /// Swaps the active harness, discarding the old one's session.
    pub fn set_kind(
        &mut self,
        kind: crate::settings::HarnessKind,
        settings: &crate::settings::AppSettings,
    ) {
        let next = Harness::from_settings(kind, settings);
        // Replace in place so existing clones (test doubles) stay valid.
        *self = next;
    }

    /// Toggles automatic permission approval; ignored by the Pi harness.
    pub fn set_auto_approve(&mut self, on: bool) {
        if let Harness::Acp(r) = self {
            r.set_auto_approve(on);
        }
    }

    /// Applies a model / thinking-level change to a live ACP session.
    pub fn set_config_option(&self, config_id: &str, value: &str) {
        if let Harness::Acp(r) = self {
            r.set_config_option(config_id, value);
        }
    }

    /// Applies an ACP mode change to a live session.
    pub fn set_mode(&self, mode_id: &str) {
        if let Harness::Acp(r) = self {
            r.set_mode(mode_id);
        }
    }

    /// Answers a pending permission request; a no-op for the Pi harness.
    pub fn respond_permission(&self, request_id: u64, option_id: &str) {
        if let Harness::Acp(r) = self {
            r.respond_permission(request_id, option_id);
        }
    }

    /// Sends one prompt on the active harness.
    ///
    /// ACP selections are reconciled safely against the session's advertised
    /// catalogs inside the controller before dispatching `session/prompt`.
    pub fn spawn_prompt(
        &mut self,
        prompt: String,
        attached_files: Vec<(String, String)>,
        model: Option<String>,
        thinking_level: Option<String>,
        mode: Option<String>,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Harness::Pi(runner) => {
                runner.spawn_prompt(prompt, attached_files, model, thinking_level, mode, tx)
            }
            Harness::Acp(runner) => {
                runner.spawn_prompt(prompt, attached_files, model, thinking_level, mode, tx)
            }
        }
    }
}

/// Commands sent to the ACP controller task.
enum AcpCommand {
    /// Eagerly spawn the ACP child and hydrate session catalogs.
    Warmup { gen: u64 },
    /// Apply harness settings; drops the process so the next prompt rebinds.
    Configure {
        command: String,
        args: Vec<String>,
        workspace: std::path::PathBuf,
        auto_approve: bool,
    },
    /// Update auto-approval on the active session immediately.
    SetAutoApprove(bool),
    /// Discard the conversation and start a new ACP session.
    NewSession,
    /// Cancel any currently running ACP turn (`session/cancel`).
    Cancel,
    /// Answer a parked permission request.
    RespondPermission { request_id: u64, option_id: String },
    /// Change model / thinking level on the live session.
    SetConfigOption { config_id: String, value: String },
    /// Change ACP mode on the live session.
    SetMode { mode_id: String },
    /// Send a prompt after reconciling selected model / thinking / mode.
    Prompt {
        prompt: String,
        attached_files: Vec<(String, String)>,
        model: Option<String>,
        thinking_level: Option<String>,
        mode: Option<String>,
        gen: u64,
    },
}

/// Drives a persistent ACP process (`agy acp` / `opencode acp`) across turns.
///
/// The child process is owned by a detached controller task that never blocks on
/// in-flight turns, so permissions, mode switches, and cancellations execute
/// concurrently without deadlocking.
pub struct AgyAcpRunner {
    tx: Option<mpsc::UnboundedSender<AcpCommand>>,
    generation: u64,
    workspace_dir: std::path::PathBuf,
    acp_command: String,
    acp_args: Vec<String>,
    auto_approve: bool,
}

impl AgyAcpRunner {
    pub fn new(
        acp_command: String,
        acp_args: Vec<String>,
        workspace_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            tx: None,
            generation: 1,
            workspace_dir,
            acp_command,
            acp_args,
            auto_approve: false,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn set_auto_approve(&mut self, on: bool) {
        self.auto_approve = on;
        self.send(AcpCommand::SetAutoApprove(on));
    }

    /// Rebinds the harness: a new workspace or command needs a fresh process.
    pub fn set_workspace_dir(&mut self, dir: impl Into<std::path::PathBuf>) {
        let dir = dir.into();
        if self.workspace_dir == dir {
            return;
        }
        self.workspace_dir = dir;
        self.generation = self.generation.wrapping_add(1);
        self.send_config();
    }

    /// Drops the current conversation; the next prompt starts a new session.
    pub fn new_session(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(tx) = &self.tx {
            let _ = tx.send(AcpCommand::NewSession);
        }
    }

    pub fn abort(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(tx) = &self.tx {
            let _ = tx.send(AcpCommand::Cancel);
        }
    }

    pub fn warmup(&mut self, tx: mpsc::UnboundedSender<(u64, StreamEvent)>) {
        let gen = self.generation;
        let cmd_tx = self.ensure_controller(tx);
        let _ = cmd_tx.send(AcpCommand::Warmup { gen });
    }

    fn send(&self, cmd: AcpCommand) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(cmd);
        }
    }

    fn send_config(&self) {
        self.send(AcpCommand::Configure {
            command: self.acp_command.clone(),
            args: self.acp_args.clone(),
            workspace: self.workspace_dir.clone(),
            auto_approve: self.auto_approve,
        });
    }

    /// Answers a pending permission request.
    pub fn respond_permission(&self, request_id: u64, option_id: &str) {
        self.send(AcpCommand::RespondPermission {
            request_id,
            option_id: option_id.to_string(),
        });
    }

    /// Applies a model / thinking-level change to the live session.
    pub fn set_config_option(&self, config_id: &str, value: &str) {
        self.send(AcpCommand::SetConfigOption {
            config_id: config_id.to_string(),
            value: value.to_string(),
        });
    }

    /// Applies an ACP mode change to the live session.
    pub fn set_mode(&self, mode_id: &str) {
        self.send(AcpCommand::SetMode {
            mode_id: mode_id.to_string(),
        });
    }

    /// Sends one prompt. Starts the controller task on first use.
    pub fn spawn_prompt(
        &mut self,
        prompt: String,
        attached_files: Vec<(String, String)>,
        model: Option<String>,
        thinking_level: Option<String>,
        mode: Option<String>,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.generation = self.generation.wrapping_add(1);
        let gen = self.generation;

        let cmd_tx = self.ensure_controller(tx);
        cmd_tx
            .send(AcpCommand::Prompt {
                prompt,
                attached_files,
                model,
                thinking_level,
                mode,
                gen,
            })
            .map_err(|_| "ACP controller task stopped")?;
        Ok(())
    }

    /// Starts the controller task if needed and returns its command channel.
    fn ensure_controller(
        &mut self,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> mpsc::UnboundedSender<AcpCommand> {
        if let Some(existing) = &self.tx {
            if !existing.is_closed() {
                return existing.clone();
            }
        }

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<AcpCommand>();
        let initial = AcpSettings {
            command: self.acp_command.clone(),
            args: self.acp_args.clone(),
            workspace: self.workspace_dir.clone(),
            auto_approve: self.auto_approve,
        };
        tokio::spawn(acp_controller(initial, cmd_rx, tx));
        self.tx = Some(cmd_tx.clone());
        cmd_tx
    }
}

impl Default for AgyAcpRunner {
    fn default() -> Self {
        Self::new(
            String::new(),
            vec!["acp".to_string()],
            std::env::current_dir().unwrap_or_default(),
        )
    }
}

/// Harness settings the controller task owns.
struct AcpSettings {
    command: String,
    args: Vec<String>,
    workspace: std::path::PathBuf,
    auto_approve: bool,
}

async fn ensure_acp_client(
    client: &mut Option<std::sync::Arc<crate::acp::AcpClient>>,
    settings: &AcpSettings,
    gen: u64,
    tx: &mpsc::UnboundedSender<(u64, StreamEvent)>,
) -> Option<std::sync::Arc<crate::acp::AcpClient>> {
    if let Some(existing) = client.as_ref() {
        if existing.is_alive() {
            return Some(existing.clone());
        }
        existing.shutdown().await;
        *client = None;
    }

    match crate::acp::AcpClient::spawn(
        &settings.command,
        &settings.args,
        &settings.workspace,
        settings.auto_approve,
        gen,
        tx.clone(),
    )
    .await
    {
        Ok(c) => {
            let arc = std::sync::Arc::new(c);
            let _ = tx.send((gen, StreamEvent::SessionConfig(arc.config.clone())));
            *client = Some(arc.clone());
            Some(arc)
        }
        Err(e) => {
            let _ = tx.send((
                gen,
                StreamEvent::Error(format!("failed to start ACP agent: {e}")),
            ));
            let _ = tx.send((gen, StreamEvent::Done));
            None
        }
    }
}

/// Applies UI selections only when supported by the connected ACP server.
async fn reconcile_acp_selections(
    client: &crate::acp::AcpClient,
    model: Option<&str>,
    thinking: Option<&str>,
    mode: Option<&str>,
) {
    if let Some(m) = model.filter(|s| !s.is_empty() && *s != "default") {
        let supported = client.config.models.iter().any(|(id, _)| id == m);
        if supported {
            let _ = client.set_config_option("model", m).await;
        }
    }
    if let Some(t) = thinking.filter(|s| !s.is_empty()) {
        let supported = client.config.thinking_levels.iter().any(|lvl| lvl == t);
        if supported {
            let _ = client.set_config_option("thought_level", t).await;
        }
    }
    if let Some(md) = mode.filter(|s| !s.is_empty()) {
        let resolved_mode = if client.config.modes.iter().any(|(id, _)| id == md) {
            Some(md)
        } else if md != "plan" && client.config.modes.iter().any(|(id, _)| id == "build") {
            Some("build")
        } else {
            None
        };
        if let Some(target_mode) = resolved_mode {
            let _ = client.set_mode(target_mode).await;
        }
    }
}

/// Owns the ACP child process without blocking concurrent commands while a prompt streams.
async fn acp_controller(
    mut settings: AcpSettings,
    mut cmd_rx: mpsc::UnboundedReceiver<AcpCommand>,
    tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
) {
    let mut client: Option<std::sync::Arc<crate::acp::AcpClient>> = None;
    let mut in_flight: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AcpCommand::Warmup { gen } => {
                let _ = ensure_acp_client(&mut client, &settings, gen, &tx).await;
            }
            AcpCommand::Configure {
                command,
                args,
                workspace,
                auto_approve,
            } => {
                settings = AcpSettings {
                    command,
                    args,
                    workspace,
                    auto_approve,
                };
                if let Some(handle) = in_flight.take() {
                    handle.abort();
                }
                if let Some(c) = client.take() {
                    c.shutdown().await;
                }
            }
            AcpCommand::SetAutoApprove(on) => {
                settings.auto_approve = on;
                if let Some(c) = client.as_ref() {
                    c.set_auto_approve(on);
                }
            }
            AcpCommand::NewSession => {
                if let Some(handle) = in_flight.take() {
                    handle.abort();
                }
                if let Some(c) = client.take() {
                    c.shutdown().await;
                }
            }
            AcpCommand::Cancel => {
                if let Some(c) = client.as_ref() {
                    let _ = c.cancel().await;
                }
            }
            AcpCommand::RespondPermission {
                request_id,
                option_id,
            } => {
                if let Some(c) = client.as_ref() {
                    let _ = c.respond_permission(request_id, &option_id).await;
                }
            }
            AcpCommand::SetConfigOption { config_id, value } => {
                if let Some(c) = client.as_ref() {
                    let supported = match config_id.as_str() {
                        "model" => {
                            c.config.models.is_empty()
                                || c.config.models.iter().any(|(id, _)| id == &value)
                        }
                        "thought_level" => c.config.thinking_levels.iter().any(|l| l == &value),
                        _ => true,
                    };
                    if supported {
                        let _ = c.set_config_option(&config_id, &value).await;
                    }
                }
            }
            AcpCommand::SetMode { mode_id } => {
                if let Some(c) = client.as_ref() {
                    let target = if c.config.modes.iter().any(|(id, _)| id == &mode_id) {
                        Some(mode_id)
                    } else if mode_id != "plan"
                        && c.config.modes.iter().any(|(id, _)| id == "build")
                    {
                        Some("build".to_string())
                    } else {
                        None
                    };
                    if let Some(m) = target {
                        let _ = c.set_mode(&m).await;
                    }
                }
            }
            AcpCommand::Prompt {
                prompt,
                attached_files,
                model,
                thinking_level,
                mode,
                gen,
            } => {
                if let Some(handle) = in_flight.take() {
                    if !handle.is_finished() {
                        if let Some(c) = client.as_ref() {
                            let _ = c.cancel().await;
                        }
                        handle.abort();
                    }
                }
                let Some(active) = ensure_acp_client(&mut client, &settings, gen, &tx).await else {
                    continue;
                };
                reconcile_acp_selections(
                    &active,
                    model.as_deref(),
                    thinking_level.as_deref(),
                    mode.as_deref(),
                )
                .await;

                let blocks = crate::acp::build_prompt_blocks(&prompt, &attached_files);
                let tx_turn = tx.clone();
                in_flight = Some(tokio::spawn(async move {
                    if let Err(e) = active.prompt(blocks, gen, tx_turn.clone()).await {
                        let _ = tx_turn.send((gen, StreamEvent::Error(e)));
                    }
                    let _ = tx_turn.send((gen, StreamEvent::Done));
                }));
            }
        }
    }

    if let Some(handle) = in_flight.take() {
        handle.abort();
    }
    if let Some(c) = client.take() {
        c.shutdown().await;
    }
}

/// Directory holding per-session state for the JSON harness.
pub fn session_state_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("pi-overlay-sessions")
}

/// Generates a fresh session id, e.g. `pi-1758000000-42`.
///
/// The counter guarantees uniqueness even for bursts within the same second,
/// which matters because every workspace change mints a new session.
pub fn new_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("pi-{secs}-{n}")
}

/// Model ids the Pi harness can be pointed at.
///
/// Read from the user's local Pi config so the composer offers the same choices
/// the CLI would.
pub fn query_pi_models_sync() -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        for rel in [".pi/agent/models.json", ".pi/agent/models-store.json"] {
            let path = std::path::PathBuf::from(&home).join(rel);
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
                continue;
            };
            // Both files nest models under provider objects.
            let providers = val
                .get("providers")
                .and_then(|p| p.as_object())
                .or_else(|| val.as_object());
            let Some(providers) = providers else { continue };

            for (provider_id, provider) in providers {
                let Some(models) = provider.get("models").and_then(|m| m.as_array()) else {
                    continue;
                };
                for model in models {
                    let Some(id) = model.get("id").and_then(|i| i.as_str()) else {
                        continue;
                    };
                    let name = model.get("name").and_then(|n| n.as_str()).unwrap_or(id);
                    let qualified = if id.starts_with(provider_id) {
                        id.to_string()
                    } else {
                        format!("{provider_id}/{id}")
                    };
                    if !results.iter().any(|(existing, _)| existing == &qualified) {
                        results.push((qualified, format!("{provider_id}/{name}")));
                    }
                }
            }
        }
    }

    results.insert(
        0,
        ("default".to_string(), "Auto (system preset)".to_string()),
    );
    results
}

pub fn resolve_pi_binary() -> std::path::PathBuf {
    resolve_pi_binary_in(
        std::env::var_os("PATH").as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

fn resolve_pi_binary_in(
    path_env: Option<&std::ffi::OsStr>,
    home: Option<&str>,
) -> std::path::PathBuf {
    if let Some(path) = path_env {
        for dir in std::env::split_paths(path) {
            let candidate = dir.join("pi");
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    if let Some(home) = home {
        let home_path = std::path::PathBuf::from(home);
        let candidates = [
            home_path.join(".bun/bin/pi"),
            home_path.join(".local/bin/pi"),
            home_path.join(".cargo/bin/pi"),
            home_path.join(".local/share/mise/shims/pi"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    for sys in [
        "/home/linuxbrew/.linuxbrew/bin/pi",
        "/usr/local/bin/pi",
        "/usr/bin/pi",
    ] {
        let p = std::path::PathBuf::from(sys);
        if p.is_file() {
            return p;
        }
    }

    std::path::PathBuf::from("pi")
}

impl PiRunner {
    pub fn new() -> Self {
        Self {
            child: None,
            generation: 1,
            session_id: new_session_id(),
            workspace_dir: std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from(".")),
        }
    }

    /// Session id reused across turns; exposed for tests.
    #[cfg(test)]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Directory the harness runs in; exposed for tests.
    #[cfg(test)]
    pub fn workspace_dir(&self) -> &std::path::Path {
        &self.workspace_dir
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Starts a fresh conversation: new session id, previous child reaped.
    pub fn new_session(&mut self) {
        self.abort();
        self.session_id = new_session_id();
    }

    /// Points the harness at a different workspace (the agent's `cwd`).
    ///
    /// Resets the session because a conversation is bound to one directory.
    pub fn set_workspace_dir(&mut self, dir: impl Into<std::path::PathBuf>) {
        let dir = dir.into();
        if self.workspace_dir == dir {
            return;
        }
        self.workspace_dir = dir;
        self.new_session();
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
        attached_files: Vec<(String, String)>,
        model: Option<String>,
        thinking_level: Option<String>,
        mode: Option<String>,
        tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Abort previous session before launching new prompt
        self.abort();
        let gen = self.generation;
        let session_id = self.session_id.clone();
        let workspace_dir = self.workspace_dir.clone();

        // Prepend user toolchain directories to PATH for pi and tool executions
        let augmented_path = augmented_path();

        if !workspace_dir.is_dir() {
            return Err(
                format!("workspace directory not found: {}", workspace_dir.display()).into(),
            );
        }

        let pi_bin = resolve_pi_binary();
        let mut cmd = Command::new(&pi_bin);
        cmd.env("PATH", &augmented_path).current_dir(&workspace_dir);

        let args = build_pi_command_args(
            &session_state_dir(),
            &session_id,
            &prompt,
            model.as_deref(),
            thinking_level.as_deref(),
            mode.as_deref(),
            &attached_files,
        );

        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

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
                if trimmed.is_empty()
                    || trimmed.starts_with("Warning: No project session found with id")
                {
                    continue;
                }
                let _ = tx_err.send((gen, StreamEvent::Error(line)));
            }
        });

        self.child = Some(child);
        Ok(())
    }
}

/// PATH with the user's toolchain directories prepended, so `pi` and the
/// commands it runs resolve the same binaries as the user's shell.
pub fn augmented_path() -> String {
    let current_path = std::env::var("PATH").unwrap_or_default();
    match std::env::var("HOME") {
        Ok(home) => format!(
            "{h}/.local/share/mise/installs/node/lts/bin:{h}/.local/share/mise/shims:{h}/.bun/bin:{h}/.local/bin:{h}/.cargo/bin:/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{current_path}",
            h = home
        ),
        Err(_) => format!("/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{current_path}"),
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

    // 1. ACP-style JSON-RPC notifications can appear on the same stream.
    if let Some(crate::acp::AcpMessage::Notification { params, .. }) =
        crate::acp::classify_message(trimmed)
    {
        if let Some(ev) = crate::acp::session_update_to_event(&params) {
            if tx.send((gen, ev)).is_err() {
                return Err(());
            }
            return Ok(());
        }
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
                    let id = v
                        .get("toolCallId")
                        .and_then(|n| n.as_str())
                        .unwrap_or("tool_call")
                        .to_string();
                    let name = v
                        .get("toolName")
                        .and_then(|n| n.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let args = v.get("args").map(|a| a.to_string()).unwrap_or_default();
                    let _ = tx.send((gen, StreamEvent::ToolStart { id, name, args }));
                }
                "tool_execution_end" => {
                    let id = v
                        .get("toolCallId")
                        .and_then(|n| n.as_str())
                        .unwrap_or("tool_call")
                        .to_string();
                    let name = v
                        .get("toolName")
                        .and_then(|n| n.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let is_error = v.get("isError").and_then(|b| b.as_bool()).unwrap_or(false);
                    let duration_ms = v.get("durationMs").and_then(|d| d.as_u64()).unwrap_or(0);
                    let result_str = if let Some(res) = v.get("result") {
                        extract_tool_result_text(res)
                    } else {
                        String::new()
                    };
                    let _ = tx.send((
                        gen,
                        StreamEvent::ToolEnd {
                            id,
                            name,
                            result: result_str,
                            is_error,
                            duration_ms,
                        },
                    ));
                }
                "message_end" | "turn_end" => {
                    // Tool results and final assistant contents are NOT emitted here:
                    // assistant text is streamed live via message_update deltas, and tool results
                    // belong solely in ToolEnd cards. Extract metrics only.
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

/// Builds the `pi -p --mode json` argument vector.
///
/// Attachments are passed as `@<path>` positional arguments after `--` so the
/// harness reads them from disk; inline (non-path) content is inlined into the
/// prompt text instead.
pub fn build_pi_command_args(
    session_dir: &std::path::Path,
    session_id: &str,
    prompt: &str,
    model: Option<&str>,
    thinking_level: Option<&str>,
    mode: Option<&str>,
    attached_files: &[(String, String)],
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--mode".to_string(),
        "json".to_string(),
        "--session-dir".to_string(),
        session_dir.to_string_lossy().to_string(),
        "--session-id".to_string(),
        session_id.to_string(),
    ];

    if let Some(m) = model {
        if m != "default" && !m.trim().is_empty() {
            args.push("--model".to_string());
            args.push(m.to_string());
        }
    }

    if let Some(th) = thinking_level {
        if !th.trim().is_empty() {
            args.push("--thinking".to_string());
            args.push(th.to_string());
        }
    }

    if let Some(md) = mode {
        match md {
            "ask" => {
                args.push("--exclude-tools".to_string());
                args.push("edit,write".to_string());
                args.push("--append-system-prompt".to_string());
                args.push("Mode: Ask. Answer questions and analyze code without modifying or creating files.".to_string());
            }
            "edit" => {
                args.push("--append-system-prompt".to_string());
                args.push("Mode: Edit. Focus on directly modifying and improving the code in the workspace.".to_string());
            }
            "plan" => {
                args.push("--exclude-tools".to_string());
                args.push("edit,write".to_string());
                args.push("--append-system-prompt".to_string());
                args.push("Mode: Plan. Create clear architectural plans and implementation breakdowns before making changes.".to_string());
            }
            _ => {}
        }
    }

    let has_screen = has_screen_attachment(attached_files);
    if has_screen {
        let dims = attached_files
            .iter()
            .filter(|(n, _)| crate::capture::is_image_path(n))
            .find_map(|(n, _)| crate::capture::png_dimensions(std::path::Path::new(n)))
            .map(|(w, h)| format!(" Screenshot dimensions: {w}x{h}px."))
            .unwrap_or_default();
        args.push("--append-system-prompt".to_string());
        args.push(format!("{}{}", SCREEN_MARKUP_HINT.trim(), dims));
    }

    let mut prompt_text = if has_screen {
        prompt.replace("@screen", "[attached screenshot]")
    } else {
        prompt.to_string()
    };

    // `pi` treats any positional argument starting with `@` as `@file`, so guard
    // prompts like `@main.rs explain` or `@screen ...` from being parsed as a path.
    if prompt_text.trim_start().starts_with('@') {
        prompt_text = format!("Use the referenced context for this request: {prompt_text}");
    }

    // Path-backed attachments become `@file` arguments; inline blobs are inlined.
    let mut file_args: Vec<String> = Vec::new();
    for (name, content) in attached_files {
        if std::path::Path::new(name).is_file() {
            file_args.push(format!("@{name}"));
        } else if !content.trim().is_empty() {
            prompt_text.push_str(&format!("\n\n[Attached: {name}]\n```\n{content}\n```\n"));
        }
    }

    args.push("--".to_string());
    args.extend(file_args);
    args.push(prompt_text);
    args
}

/// Teaches the agent how to highlight screen regions for the user.
pub const SCREEN_MARKUP_HINT: &str = "\n\n[Screen context attached. To point at UI elements or regions on the user's screen, \
emit inline tags anywhere in your reply using normalized 0-1000 coordinates (origin top-left, x=0..1000, y=0..1000), e.g. \
[[markup: box scale=1000 x=120 y=300 w=250 h=80 label=\"Retry button\"]] or \
[[markup: arrow scale=1000 x1=500 y1=500 x2=250 y2=300 label=\"Click here\"]]. \
Tags are stripped from prose and rendered live as a click-through overlay on the user's screen.]";

/// True when at least one attachment is a screen capture.
pub fn has_screen_attachment(attached_files: &[(String, String)]) -> bool {
    attached_files
        .iter()
        .any(|(name, _)| crate::capture::is_image_path(name))
}

pub fn extract_tool_result_text(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(content_arr) = v.get("content").and_then(|c| c.as_array()) {
        let texts: Vec<String> = content_arr
            .iter()
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .map(|t| t.to_string())
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    if let Some(stdout) = v.get("stdout").and_then(|o| o.as_str()) {
        let mut out = stdout.to_string();
        if let Some(stderr) = v.get("stderr").and_then(|e| e.as_str()) {
            if !stderr.is_empty() {
                out.push('\n');
                out.push_str(stderr);
            }
        }
        return out;
    }
    if let Some(output) = v.get("output").and_then(|o| o.as_str()) {
        return output.to_string();
    }
    if let Some(details) = v.get("details").and_then(|d| d.as_str()) {
        return details.to_string();
    }
    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        return text.to_string();
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_initial_state() {
        let runner = PiRunner::new();
        assert_eq!(runner.generation(), 1);
        assert!(runner.child.is_none());
        assert!(runner.session_id().starts_with("pi-"));
    }

    #[test]
    fn test_resolve_pi_binary() {
        let dir = std::env::temp_dir().join(format!("pi-overlay-test-bin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake_pi = dir.join("pi");
        std::fs::write(&fake_pi, "#!/bin/sh\n").unwrap();

        let resolved = resolve_pi_binary_in(Some(dir.as_os_str()), None);
        assert_eq!(resolved, fake_pi);

        let fallback =
            resolve_pi_binary_in(Some(std::ffi::OsStr::new("")), Some("/nonexistent-home"));
        assert!(
            fallback.is_file() || fallback == std::path::Path::new("pi"),
            "unexpected fallback {:?}",
            fallback
        );

        let _ = std::fs::remove_dir_all(&dir);
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
    fn test_new_session_changes_id_and_keeps_workspace() {
        let mut runner = PiRunner::new();
        let first = runner.session_id().to_string();
        let ws = runner.workspace_dir().to_path_buf();
        runner.new_session();
        assert_ne!(runner.session_id(), first, "new session must mint a new id");
        assert_eq!(runner.workspace_dir(), ws);
    }

    #[test]
    fn test_set_workspace_dir_resets_session() {
        let mut runner = PiRunner::new();
        let before = runner.session_id().to_string();
        let target = std::env::temp_dir();
        runner.set_workspace_dir(target.clone());
        assert_eq!(runner.workspace_dir(), target.as_path());
        assert_ne!(
            runner.session_id(),
            before,
            "changing workspace must start a new session"
        );
    }

    #[test]
    fn test_set_same_workspace_keeps_session() {
        let mut runner = PiRunner::new();
        let ws = runner.workspace_dir().to_path_buf();
        let before = runner.session_id().to_string();
        runner.set_workspace_dir(ws);
        assert_eq!(runner.session_id(), before);
    }

    #[test]
    fn test_new_session_id_is_unique_within_a_second() {
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b, "bursts in one second must still be unique");
        assert!(a.starts_with("pi-"));
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
                StreamEvent::ToolStart {
                    id: "call_1".to_string(),
                    name: "bash".to_string(),
                    args: r#"{"command":"ls"}"#.to_string()
                }
            )
        );

        let ev4 = rx.try_recv().unwrap();
        assert_eq!(ev4, (10, StreamEvent::Done));
    }

    #[test]
    fn test_parse_tool_execution_end() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let gen = 20;

        let json_end = r#"{"type":"tool_execution_end","toolCallId":"call_123","toolName":"read","result":{"content":[{"type":"text","text":"file contents"}],"details":{}},"isError":false,"durationMs":45}"#;
        assert!(parse_and_dispatch_line(json_end, gen, &tx).is_ok());

        let (g, ev) = rx.try_recv().unwrap();
        assert_eq!(g, 20);
        match ev {
            StreamEvent::ToolEnd {
                id,
                name,
                result,
                is_error,
                duration_ms,
            } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "read");
                assert_eq!(result, "file contents");
                assert!(!is_error);
                assert_eq!(duration_ms, 45);
            }
            other => panic!("Expected ToolEnd, got {:?}", other),
        }
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
            StreamEvent::ToolStart { name, args, .. } => {
                assert_eq!(name, "edit");
                assert!(args.contains("Xin chào thế giới 🦀"));
            }
            other => panic!("Expected ToolStart, got {:?}", other),
        }

        // Verify tool call 2
        let (g2, ev2) = rx.try_recv().unwrap();
        assert_eq!(g2, 42);
        match ev2 {
            StreamEvent::ToolStart { name, args, .. } => {
                assert_eq!(name, "bash");
                assert_eq!(args, "");
            }
            other => panic!("Expected ToolStart, got {:?}", other),
        }

        // Verify tool call 3
        let (g3, ev3) = rx.try_recv().unwrap();
        assert_eq!(g3, 42);
        match ev3 {
            StreamEvent::ToolStart { name, args, .. } => {
                assert_eq!(name, "read");
                assert!(args.contains("tệp_tin.txt"));
            }
            other => panic!("Expected ToolStart, got {:?}", other),
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

    #[test]
    fn test_tool_output_does_not_leak_into_assistant_text() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let gen = 99;

        // 1. Tool execution end sends ToolEnd
        let tool_end_json = r#"{"type":"tool_execution_end","toolCallId":"call_99","toolName":"bash","result":"total 42\nfile.txt","isError":false,"durationMs":10}"#;
        assert!(parse_and_dispatch_line(tool_end_json, gen, &tx).is_ok());

        let (_, ev) = rx.try_recv().expect("Should have received ToolEnd");
        match ev {
            StreamEvent::ToolEnd { id, result, .. } => {
                assert_eq!(id, "call_99");
                assert_eq!(result, "total 42\nfile.txt");
            }
            other => panic!("Expected ToolEnd, got {:?}", other),
        }

        // 2. toolResult message_end must NOT leak into TextDelta
        let tool_result_msg_end = r#"{"type":"message_end","message":{"role":"toolResult","content":[{"type":"text","text":"total 42\nfile.txt"}]}}"#;
        assert!(parse_and_dispatch_line(tool_result_msg_end, gen, &tx).is_ok());

        // 3. turn_end with toolResults must NOT leak into TextDelta
        let turn_end_json = r#"{"type":"turn_end","message":{"role":"assistant","content":[{"type":"toolCall","id":"call_99","name":"bash","arguments":{}}]},"toolResults":[{"role":"toolResult","toolCallId":"call_99","toolName":"bash","content":[{"type":"text","text":"total 42\nfile.txt"}],"isError":false,"durationMs":10}]}"#;
        assert!(parse_and_dispatch_line(turn_end_json, gen, &tx).is_ok());

        // Nothing must have leaked to TextDelta
        assert!(
            rx.try_recv().is_err(),
            "Tool output must NOT leak into receiver"
        );
    }

    #[test]
    fn test_build_pi_command_args_matrix() {
        let dummy_dir = std::path::Path::new("/tmp/test-sessions");
        let no_files: Vec<(String, String)> = Vec::new();

        // 1. Default parameters: no model override, default prompt
        let args1 = build_pi_command_args(
            dummy_dir,
            "pi-1",
            "Hello",
            Some("default"),
            Some("medium"),
            Some("agent"),
            &no_files,
        );
        assert_eq!(args1[0], "-p");
        assert_eq!(args1[1], "--mode");
        assert_eq!(args1[2], "json");
        assert_eq!(args1[3], "--session-dir");
        assert_eq!(args1[4], "/tmp/test-sessions");
        assert_eq!(args1[5], "--session-id");
        assert_eq!(args1[6], "pi-1");
        assert!(!args1.contains(&"--model".to_string()));
        assert_eq!(
            args1[args1.iter().position(|r| r == "--thinking").unwrap() + 1],
            "medium"
        );
        assert_eq!(args1[args1.len() - 2], "--");
        assert_eq!(args1[args1.len() - 1], "Hello");

        // 2. Specific model and ask mode
        let args2 = build_pi_command_args(
            dummy_dir,
            "pi-2",
            "Review",
            Some("claude-3-7-sonnet"),
            Some("high"),
            Some("ask"),
            &no_files,
        );
        assert_eq!(
            args2[args2.iter().position(|r| r == "--model").unwrap() + 1],
            "claude-3-7-sonnet"
        );
        assert_eq!(
            args2[args2.iter().position(|r| r == "--thinking").unwrap() + 1],
            "high"
        );
        assert_eq!(
            args2[args2.iter().position(|r| r == "--exclude-tools").unwrap() + 1],
            "edit,write"
        );

        // 3. Plan mode disables write tools
        let args3 = build_pi_command_args(
            dummy_dir,
            "pi-3",
            "Plan arch",
            None,
            Some("off"),
            Some("plan"),
            &no_files,
        );
        assert_eq!(
            args3[args3.iter().position(|r| r == "--exclude-tools").unwrap() + 1],
            "edit,write"
        );
        assert_eq!(
            args3[args3.iter().position(|r| r == "--thinking").unwrap() + 1],
            "off"
        );

        // 4. Edit mode keeps tools enabled
        let args4 = build_pi_command_args(
            dummy_dir,
            "pi-4",
            "Refactor",
            Some("gpt-4o"),
            None,
            Some("edit"),
            &no_files,
        );
        assert!(!args4.contains(&"--exclude-tools".to_string()));
        assert_eq!(
            args4[args4.iter().position(|r| r == "--model").unwrap() + 1],
            "gpt-4o"
        );
    }

    #[test]
    fn test_build_pi_command_args_passes_existing_attachment_paths() {
        let temp = std::env::temp_dir().join("pi-overlay-test-attach");
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("notes.txt");
        std::fs::write(&file, "hello").unwrap();

        let attached = vec![(file.to_string_lossy().to_string(), String::new())];
        let args = build_pi_command_args(
            std::path::Path::new("/tmp/sessions"),
            "pi-9",
            "Summarise",
            None,
            None,
            None,
            &attached,
        );

        let dash = args.iter().position(|a| a == "--").expect("must have --");
        assert_eq!(args[dash + 1], format!("@{}", file.display()));
        assert_eq!(args[dash + 2], "Summarise");
        assert_eq!(args.last().unwrap(), "Summarise");
    }

    #[test]
    fn test_build_pi_command_args_inlines_non_path_content() {
        let attached = vec![("pasted snippet".to_string(), "let x = 1;".to_string())];
        let args = build_pi_command_args(
            std::path::Path::new("/tmp/sessions"),
            "pi-10",
            "Explain",
            None,
            None,
            None,
            &attached,
        );
        let last = args.last().unwrap();
        assert!(last.contains("[Attached: pasted snippet]"));
        assert!(last.contains("let x = 1;"));
        assert!(last.contains("Explain"));
        // No `@` argument for inline content that has no file on disk.
        assert!(!args.iter().any(|a| a.starts_with("@pasted")));
    }

    #[test]
    fn test_screen_attachment_appends_markup_hint() {
        let attached = vec![(
            "/tmp/pi-overlay-captures/screen.png".to_string(),
            String::new(),
        )];
        assert!(has_screen_attachment(&attached));

        let args = build_pi_command_args(
            std::path::Path::new("/tmp/sessions"),
            "pi-11",
            "@screen What is on screen?",
            None,
            None,
            None,
            &attached,
        );
        let sys_idx = args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .expect("markup hint belongs in --append-system-prompt");
        assert!(args[sys_idx + 1].contains("[[markup: box"));

        let last = args.last().unwrap();
        assert!(!last.starts_with('@'), "message arg must not start with @");
        assert!(
            !last.contains("[[markup: box"),
            "markup hint must not pollute user prompt"
        );
        assert!(last.contains("What is on screen?"));
    }

    #[test]
    fn test_pi_prompt_starting_with_at_mention_is_guarded() {
        let args = build_pi_command_args(
            std::path::Path::new("/tmp/sessions"),
            "pi-12",
            "@src/main.rs explain this",
            None,
            None,
            None,
            &[],
        );
        let last = args.last().unwrap();
        assert!(!last.starts_with('@'));
        assert!(last.contains("@src/main.rs explain this"));
    }

    #[test]
    fn test_has_screen_attachment_false_for_text_files() {
        let attached = vec![("notes.txt".to_string(), String::new())];
        assert!(!has_screen_attachment(&attached));
        assert!(!has_screen_attachment(&[]));
    }

    #[test]
    fn test_extract_tool_result_text_formats() {
        // Plain string
        let v1 = serde_json::json!("file contents");
        assert_eq!(extract_tool_result_text(&v1), "file contents");

        // stdout and stderr object
        let v2 = serde_json::json!({"stdout": "line 1\nline 2", "stderr": "warn: low memory"});
        assert_eq!(
            extract_tool_result_text(&v2),
            "line 1\nline 2\nwarn: low memory"
        );

        // Text field object
        let v3 = serde_json::json!({"text": "custom tool output"});
        assert_eq!(extract_tool_result_text(&v3), "custom tool output");
    }
}
