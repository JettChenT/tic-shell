use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{Mutex, mpsc, oneshot},
};

use crate::{config::Config, ui::UiEvent};

const HEARTBEAT_MODEL: &str = "gpt-5.4-mini";

#[derive(Clone)]
pub struct AcpHandle {
    inner: Arc<AcpInner>,
}

struct AcpInner {
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<u64, Pending>>,
    heart_workers: Mutex<HashMap<String, mpsc::UnboundedSender<HeartPrompt>>>,
    prompt_workers: Mutex<HashSet<String>>,
    next_id: AtomicU64,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
    state: Mutex<BridgeState>,
    config: Config,
    repo_root: PathBuf,
    current_exe: PathBuf,
}

struct Pending {
    method: String,
    workspace_key: Option<String>,
    tx: Option<oneshot::Sender<Result<Value, String>>>,
}

struct HeartPrompt {
    title: String,
    text: String,
}

#[derive(Default)]
struct BridgeState {
    initialized: bool,
    active_key: String,
    active_title: String,
    workspaces: HashMap<String, WorkspaceState>,
    sessions: HashMap<String, String>,
    fork_sessions: HashMap<String, String>,
    next_entry_id: u64,
}

#[derive(Default, Clone)]
struct WorkspaceState {
    key: String,
    title: String,
    workspace_id: Option<String>,
    root: PathBuf,
    session_id: String,
    session_ready: bool,
    session_starting: bool,
    prompt_busy: bool,
    pending_prompts: Vec<String>,
    entries: Vec<AgentEntry>,
    tools: HashMap<String, Value>,
    available_commands: Vec<AgentCommand>,
    cursors: HashSet<String>,
    fork: Option<ForkState>,
}

#[derive(Debug, Clone)]
struct ForkState {
    id: String,
    prompt: String,
    status: String,
    status_message: String,
    cua_session_id: String,
    cursor_id: String,
    cursor_theme: String,
    window_id: u64,
    active_window: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub time: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkSessionSummary {
    pub id: String,
    pub key: String,
    pub title: String,
    pub status: String,
    #[serde(rename = "statusMessage")]
    pub status_message: String,
    #[serde(rename = "cursorTheme")]
    pub cursor_theme: String,
    #[serde(rename = "cursorId")]
    pub cursor_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "windowId")]
    pub window_id: u64,
    pub selected: bool,
}

impl AcpHandle {
    pub async fn spawn(
        config: Config,
        repo_root: PathBuf,
        ui_tx: mpsc::UnboundedSender<UiEvent>,
    ) -> Result<(Self, Child)> {
        fs::create_dir_all(&config.daemon.workdir_root)
            .with_context(|| format!("create {}", config.daemon.workdir_root.display()))?;
        fs::create_dir_all(&config.daemon.data_root)
            .with_context(|| format!("create {}", config.daemon.data_root.display()))?;
        let command_spec = adapter_command_spec(&config)?;
        let mut child = Command::new(&command_spec.0);
        child
            .args(&command_spec.1)
            .current_dir(&config.daemon.workdir_root)
            .env("TIC_SHELL_ROOT", &repo_root)
            .env("PATH", bridge_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = child.spawn().context("spawn codex-acp")?;
        let stdin = child.stdin.take().context("codex-acp stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("codex-acp stdout unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("codex-acp stderr unavailable")?;
        let handle = Self {
            inner: Arc::new(AcpInner {
                stdin: Mutex::new(stdin),
                pending: Mutex::new(HashMap::new()),
                heart_workers: Mutex::new(HashMap::new()),
                prompt_workers: Mutex::new(HashSet::new()),
                next_id: AtomicU64::new(1),
                ui_tx,
                state: Mutex::new(BridgeState {
                    active_key: "workspace:default".to_string(),
                    active_title: "Workspace".to_string(),
                    ..BridgeState::default()
                }),
                config,
                repo_root,
                current_exe: std::env::current_exe()
                    .unwrap_or_else(|_| PathBuf::from("tic-daemon")),
            }),
        };
        handle.spawn_reader(stdout);
        handle.spawn_stderr(stderr);
        handle.initialize().await?;
        Ok((handle, child))
    }

    fn spawn_reader(&self, stdout: tokio::process::ChildStdout) {
        let handle = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    handle.handle_agent_message(value).await;
                }
            }
        });
    }

    fn spawn_stderr(&self, stderr: tokio::process::ChildStderr) {
        let data_root = self.inner.config.daemon.data_root.clone();
        tokio::spawn(async move {
            let log_path = data_root.join("codex-acp.log");
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(mut file) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    let _ = writeln!(file, "{} {}", chrono::Utc::now().to_rfc3339(), line);
                }
            }
        });
    }

    pub async fn initialize(&self) -> Result<()> {
        self.status("starting");
        let _ = self
            .send_wait(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    "clientCapabilities": { "fs": { "readTextFile": true, "writeTextFile": true }, "meta": { "tic-shell": true } },
                    "clientInfo": { "name": "tic-shell", "title": "tic-shell", "version": "0.1.0" }
                }),
                None,
            )
            .await;
        {
            let mut state = self.inner.state.lock().await;
            state.initialized = true;
        }
        self.new_session_for("workspace:default", "Workspace").await;
        Ok(())
    }

    pub async fn handle_ui_input(&self, message: Value) {
        let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
        let key = message
            .get("workspaceKey")
            .and_then(Value::as_str)
            .unwrap_or("workspace:default");
        let title = message
            .get("workspaceTitle")
            .and_then(Value::as_str)
            .unwrap_or(key);
        match kind {
            "workspace" => self.set_active_workspace(key, title).await,
            "prompt" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.queue_prompt(key, title, text).await;
                }
            }
            "clear" | "new" => self.reset_workspace(key, title).await,
            "cancel" => self.cancel_workspace(key).await,
            "deactivate" => self.deactivate_workspace(key).await,
            "prepare-fork-cursor" => {
                self.prepare_fork_cursor(
                    message
                        .get("forkId")
                        .or_else(|| message.get("fork_id"))
                        .and_then(Value::as_str),
                    message.get("activeWindow").cloned(),
                )
                .await;
            }
            "fork-cursor" => {
                if let Some(text) = message.get("text").and_then(Value::as_str) {
                    self.submit_fork_cursor(
                        message
                            .get("forkId")
                            .or_else(|| message.get("fork_id"))
                            .and_then(Value::as_str),
                        text,
                        message.get("activeWindow").cloned(),
                    )
                    .await;
                }
            }
            "select-fork" => {
                if let Some(id) = message.get("id").and_then(Value::as_str) {
                    self.select_fork(id).await;
                }
            }
            "dismiss-fork" => {
                if let Some(id) = message.get("id").and_then(Value::as_str) {
                    self.dismiss_fork(id).await;
                }
            }
            _ => {}
        }
    }

    async fn prepare_fork_cursor(&self, requested_id: Option<&str>, active_window: Option<Value>) {
        let active_window = match resolve_fork_active_window(active_window) {
            Ok(active_window) => active_window,
            Err(err) => {
                self.ui_tx(UiEvent::ForkComplete {
                    status: "error".into(),
                    title: "Fork cursor failed".into(),
                    body: err.to_string(),
                    fork_id: String::new(),
                });
                return;
            }
        };
        let window_id = fork_window_id(&active_window).unwrap_or(0);
        if window_id == 0 {
            self.ui_tx(UiEvent::ForkComplete {
                status: "error".into(),
                title: "Fork cursor failed".into(),
                body: "no active window to fork cursor into".into(),
                fork_id: String::new(),
            });
            return;
        }

        let id = sanitize_fork_id(requested_id)
            .unwrap_or_else(|| format!("fork-{}", chrono::Utc::now().timestamp_millis()));
        {
            let bridge = self.inner.state.lock().await;
            if bridge.fork_sessions.contains_key(&id) {
                return;
            }
        }
        let cua_session_id = id
            .trim_start_matches("fork-")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let cua_session_id = format!("fork_{cua_session_id}");
        let cursor_id = format!("tic-cua-mcp-{cua_session_id}");
        let cursor_theme = choose_cursor_theme();

        let key = format!("fork:{id}");
        let title = "Fork cursor".to_string();
        {
            let mut bridge = self.inner.state.lock().await;
            let ws = ensure_workspace(&self.inner.config, &mut bridge, &key, &title);
            ws.title = title.clone();
            ws.fork = Some(ForkState {
                id: id.clone(),
                prompt: String::new(),
                status: "prompting".to_string(),
                status_message: "Waiting for prompt".to_string(),
                cua_session_id,
                cursor_id: cursor_id.clone(),
                cursor_theme: cursor_theme.clone(),
                window_id,
                active_window,
            });
            bridge.fork_sessions.insert(id.clone(), key.clone());
            bridge.active_key = key.clone();
            bridge.active_title = title.clone();
        }
        let _ =
            crate::niri::Niri::discover().and_then(|niri| niri.set_hardware_cursor(&cursor_theme));
        self.emit_fork_sessions().await;
        self.new_session_for(&key, &title).await;
    }

    async fn submit_fork_cursor(
        &self,
        requested_id: Option<&str>,
        text: &str,
        active_window: Option<Value>,
    ) {
        let prompt = text.trim();
        if prompt.is_empty() {
            return;
        }
        let id = sanitize_fork_id(requested_id)
            .unwrap_or_else(|| format!("fork-{}", chrono::Utc::now().timestamp_millis()));
        let key = format!("fork:{id}");
        let title = fork_workspace_title(prompt);
        let exists = {
            let mut bridge = self.inner.state.lock().await;
            if let Some(ws) = bridge.workspaces.get_mut(&key) {
                ws.title = title.clone();
                if let Some(fork) = ws.fork.as_mut() {
                    fork.prompt = prompt.to_string();
                    fork.status = "queued".to_string();
                    fork.status_message = format!("Prompt: {}", compact_status(prompt));
                }
                bridge.active_key = key.clone();
                bridge.active_title = title.clone();
                true
            } else {
                false
            }
        };
        if !exists {
            self.prepare_fork_cursor(Some(&id), active_window).await;
            let mut bridge = self.inner.state.lock().await;
            if let Some(ws) = bridge.workspaces.get_mut(&key) {
                ws.title = title.clone();
                if let Some(fork) = ws.fork.as_mut() {
                    fork.prompt = prompt.to_string();
                    fork.status = "queued".to_string();
                    fork.status_message = format!("Prompt: {}", compact_status(prompt));
                }
                bridge.active_key = key.clone();
                bridge.active_title = title.clone();
            }
        }
        if let Err(err) = self.materialize_fork_cursor(&key).await {
            self.ui_tx(UiEvent::ForkComplete {
                status: "error".into(),
                title: "Fork cursor failed".into(),
                body: err.to_string(),
                fork_id: id,
            });
            return;
        }
        self.emit_fork_sessions().await;
        self.queue_prompt(&key, &title, prompt).await;
    }

    async fn materialize_fork_cursor(&self, key: &str) -> Result<()> {
        let fork = {
            let bridge = self.inner.state.lock().await;
            bridge.workspaces.get(key).and_then(|ws| ws.fork.clone())
        };
        let Some(fork) = fork else {
            return Ok(());
        };

        crate::niri::Niri::discover().and_then(|niri| {
            let result = niri.create_virtual_cursor_at_pointer(
                &fork.cursor_id,
                fork.window_id,
                &fork.cursor_theme,
            );
            let _ = niri.clear_hardware_cursor();
            result
        })?;

        let mut bridge = self.inner.state.lock().await;
        if let Some(ws) = bridge.workspaces.get_mut(key) {
            ws.cursors.insert(fork.cursor_id);
        }
        Ok(())
    }

    async fn select_fork(&self, id: &str) {
        let key_title_status = {
            let mut bridge = self.inner.state.lock().await;
            let Some(key) = bridge.fork_sessions.get(id).cloned() else {
                return;
            };
            let Some(ws) = bridge.workspaces.get(&key) else {
                return;
            };
            let status = if ws.prompt_busy {
                "thinking".to_string()
            } else {
                ws.fork
                    .as_ref()
                    .map(|fork| fork.status.clone())
                    .unwrap_or_else(|| "ready".to_string())
            };
            let title = ws.title.clone();
            bridge.active_key = key.clone();
            bridge.active_title = title.clone();
            Some((key, title, status))
        };
        if let Some((key, _title, status)) = key_title_status {
            self.snapshot(&key).await;
            self.status(&status);
            self.emit_fork_sessions().await;
        }
    }

    async fn dismiss_fork(&self, id: &str) {
        let removed = {
            let mut bridge = self.inner.state.lock().await;
            let Some(key) = bridge.fork_sessions.remove(id) else {
                return;
            };
            let was_active = bridge.active_key == key;
            let session = bridge
                .workspaces
                .get_mut(&key)
                .map(|ws| {
                    (
                        std::mem::take(&mut ws.session_id),
                        std::mem::take(&mut ws.cursors),
                    )
                })
                .unwrap_or_default();
            bridge.workspaces.remove(&key);
            if was_active {
                bridge.active_key = "workspace:default".to_string();
                bridge.active_title = "Workspace".to_string();
            }
            Some((was_active, session.0, session.1))
        };
        let Some((was_active, session, cursors)) = removed else {
            return;
        };
        if !session.is_empty() {
            let _ = self
                .notify("session/close", json!({ "sessionId": session }))
                .await;
        }
        let _ = crate::niri::Niri::discover().and_then(|niri| niri.clear_hardware_cursor());
        for cursor in cursors {
            let _ =
                crate::niri::Niri::discover().and_then(|niri| niri.destroy_virtual_cursor(&cursor));
        }
        self.emit_fork_sessions().await;
        if was_active {
            self.snapshot("workspace:default").await;
            self.status("ready");
        }
    }

    pub async fn prompt_heart(&self, key: &str, title: &str, text: String) {
        let tx = self.heart_worker_for(key).await;
        let _ = tx.send(HeartPrompt {
            title: title.to_string(),
            text,
        });
    }

    async fn heart_worker_for(&self, key: &str) -> mpsc::UnboundedSender<HeartPrompt> {
        if let Some(tx) = self.inner.heart_workers.lock().await.get(key).cloned() {
            return tx;
        }
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner
            .heart_workers
            .lock()
            .await
            .insert(key.to_string(), tx.clone());
        self.spawn_heart_worker(key.to_string(), rx);
        tx
    }

    fn spawn_heart_worker(&self, key: String, mut rx: mpsc::UnboundedReceiver<HeartPrompt>) {
        let config = self.inner.config.clone();
        let repo_root = self.inner.repo_root.clone();
        let ui_tx = self.inner.ui_tx.clone();
        tokio::spawn(async move {
            let Ok((worker, mut child)) = AcpHandle::spawn(config, repo_root, ui_tx.clone()).await
            else {
                let _ = ui_tx.send(UiEvent::Event {
                    kind: "error".into(),
                    title: "heart ACP worker".into(),
                    body: format!("failed to start {key}"),
                });
                return;
            };
            loop {
                tokio::select! {
                    Some(prompt) = rx.recv() => {
                        worker.queue_prompt(&key, &prompt.title, &prompt.text).await;
                    }
                    status = child.wait() => {
                        let _ = status;
                        let _ = ui_tx.send(UiEvent::Event {
                            kind: "error".into(),
                            title: "heart ACP worker exited".into(),
                            body: key.clone(),
                        });
                        return;
                    }
                    else => return,
                }
            }
        });
    }

    async fn set_active_workspace(&self, key: &str, title: &str) {
        {
            let mut bridge = self.inner.state.lock().await;
            bridge.active_key = key.to_string();
            bridge.active_title = title.to_string();
            ensure_workspace(&self.inner.config, &mut bridge, key, title);
        }
        self.snapshot(key).await;
        let should_start = {
            let bridge = self.inner.state.lock().await;
            bridge.initialized
                && bridge
                    .workspaces
                    .get(key)
                    .is_some_and(|ws| ws.session_id.is_empty() && !ws.session_starting)
        };
        if should_start {
            self.new_session_for(key, title).await;
        }
    }

    async fn queue_prompt(&self, key: &str, title: &str, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if trimmed == "/clear" || trimmed == "/new" {
            self.reset_workspace(key, title).await;
            return;
        }
        if trimmed == "/cancel" {
            self.cancel_workspace(key).await;
            return;
        }
        {
            let mut bridge = self.inner.state.lock().await;
            let id = next_entry_id(&mut bridge);
            let ws = ensure_workspace(&self.inner.config, &mut bridge, key, title);
            ws.title = title.to_string();
            if let Some(fork) = ws.fork.as_mut() {
                fork.status = "queued".to_string();
                fork.status_message = format!("Prompt: {}", compact_status(trimmed));
            }
            ws.entries.push(AgentEntry {
                id,
                kind: "user".to_string(),
                title: "You".to_string(),
                body: trimmed.to_string(),
                time: now_time(),
                metadata: Value::Null,
            });
            ws.pending_prompts.push(trimmed.to_string());
        }
        self.snapshot(key).await;
        self.emit_fork_sessions().await;
        let needs_session = {
            let bridge = self.inner.state.lock().await;
            bridge
                .workspaces
                .get(key)
                .is_some_and(|ws| ws.session_id.is_empty() && !ws.session_starting)
        };
        if needs_session {
            self.new_session_for(key, title).await;
        }
        self.schedule_flush_prompts(key.to_string()).await;
    }

    async fn reset_workspace(&self, key: &str, title: &str) {
        let old_session = {
            let mut bridge = self.inner.state.lock().await;
            let ws = ensure_workspace(&self.inner.config, &mut bridge, key, title);
            let old = std::mem::take(&mut ws.session_id);
            ws.session_ready = false;
            ws.session_starting = false;
            ws.prompt_busy = false;
            ws.pending_prompts.clear();
            ws.entries.clear();
            ws.tools.clear();
            ws.available_commands.clear();
            old
        };
        if !old_session.is_empty() {
            let _ = self
                .notify("session/close", json!({ "sessionId": old_session }))
                .await;
        }
        self.snapshot(key).await;
        self.new_session_for(key, title).await;
    }

    async fn cancel_workspace(&self, key: &str) {
        let session = {
            let mut bridge = self.inner.state.lock().await;
            bridge.workspaces.get_mut(key).map(|ws| {
                ws.prompt_busy = false;
                ws.session_id.clone()
            })
        };
        if let Some(session) = session.filter(|s| !s.is_empty()) {
            let _ = self
                .notify("session/cancel", json!({ "sessionId": session }))
                .await;
        }
        self.status("ready");
    }

    async fn deactivate_workspace(&self, key: &str) {
        let cursors = {
            let mut bridge = self.inner.state.lock().await;
            bridge
                .workspaces
                .get_mut(key)
                .map(|ws| std::mem::take(&mut ws.cursors))
                .unwrap_or_default()
        };
        for cursor in cursors {
            let _ =
                crate::niri::Niri::discover().and_then(|niri| niri.destroy_virtual_cursor(&cursor));
        }
    }

    async fn new_session_for(&self, key: &str, title: &str) {
        let cwd = {
            let mut bridge = self.inner.state.lock().await;
            let ws = ensure_workspace(&self.inner.config, &mut bridge, key, title);
            if ws.session_starting || !ws.session_id.is_empty() {
                return;
            }
            ws.session_starting = true;
            match prepare_workspace_root(&self.inner.repo_root, ws) {
                Ok(cwd) => cwd,
                Err(err) => {
                    ws.session_starting = false;
                    drop(bridge);
                    self.ui_tx(UiEvent::Event {
                        kind: "error".into(),
                        title: "Workspace setup failed".into(),
                        body: err.to_string(),
                    });
                    return;
                }
            }
        };
        let mcp_server = self.mcp_server_for(key).await;
        match self
            .send_wait(
                "session/new",
                session_new_params(key, &cwd, mcp_server),
                Some(key.to_string()),
            )
            .await
        {
            Ok(result) => {
                let session_id = result
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mut bridge = self.inner.state.lock().await;
                let ws = ensure_workspace(&self.inner.config, &mut bridge, key, title);
                ws.session_starting = false;
                ws.session_ready = !session_id.is_empty();
                ws.session_id = session_id.clone();
                if !session_id.is_empty() {
                    bridge.sessions.insert(session_id.clone(), key.to_string());
                }
                drop(bridge);
                self.status(if session_id.is_empty() {
                    "error"
                } else {
                    "ready"
                });
                self.schedule_flush_prompts(key.to_string()).await;
            }
            Err(err) => {
                let mut bridge = self.inner.state.lock().await;
                if let Some(ws) = bridge.workspaces.get_mut(key) {
                    ws.session_starting = false;
                    ws.session_ready = false;
                }
                drop(bridge);
                self.ui_tx(UiEvent::Event {
                    kind: "error".into(),
                    title: "session/new".into(),
                    body: err,
                });
                self.status("error");
            }
        }
    }

    async fn mcp_server_for(&self, key: &str) -> Value {
        let (workspace_id, event_log, fork) = {
            let bridge = self.inner.state.lock().await;
            let ws = bridge.workspaces.get(key);
            let workspace_id = ws.and_then(|ws| ws.workspace_id.clone());
            let fork = ws.and_then(|ws| ws.fork.clone());
            (
                workspace_id,
                self.inner.config.daemon.data_root.join("events.jsonl"),
                fork,
            )
        };
        let mut env = vec![
            json!({ "name": "TIC_SHELL_ROOT", "value": self.inner.repo_root }),
            json!({ "name": "TIC_DAEMON_EVENT_LOG", "value": event_log }),
        ];
        if let Some(id) = workspace_id {
            env.push(json!({ "name": "CUA_WORKSPACE_ID", "value": id }));
        }
        if let Some(fork) = fork {
            env.push(json!({ "name": "CUA_SESSION_ID", "value": fork.cua_session_id }));
            env.push(json!({ "name": "CUA_CURSOR_THEME", "value": fork.cursor_theme }));
        }
        json!({
            "name": "tic",
            "command": self.inner.current_exe,
            "args": ["mcp", "--event-log", event_log],
            "env": env,
        })
    }

    async fn schedule_flush_prompts(&self, key: String) {
        {
            let mut workers = self.inner.prompt_workers.lock().await;
            if !workers.insert(key.clone()) {
                return;
            }
        }
        let handle = self.clone();
        tokio::spawn(async move {
            loop {
                handle.flush_prompts(&key).await;
                let has_pending = {
                    let bridge = handle.inner.state.lock().await;
                    bridge
                        .workspaces
                        .get(&key)
                        .is_some_and(|ws| !ws.pending_prompts.is_empty())
                };
                if !has_pending {
                    handle.inner.prompt_workers.lock().await.remove(&key);
                    let has_new_pending = {
                        let bridge = handle.inner.state.lock().await;
                        bridge
                            .workspaces
                            .get(&key)
                            .is_some_and(|ws| !ws.pending_prompts.is_empty())
                    };
                    if !has_new_pending {
                        break;
                    }
                    let mut workers = handle.inner.prompt_workers.lock().await;
                    if !workers.insert(key.clone()) {
                        break;
                    }
                }
            }
        });
    }

    async fn flush_prompts(&self, key: &str) {
        let (session_id, prompt) = {
            let mut bridge = self.inner.state.lock().await;
            let Some(ws) = bridge.workspaces.get_mut(key) else {
                return;
            };
            if !ws.session_ready || ws.prompt_busy || ws.pending_prompts.is_empty() {
                return;
            }
            ws.prompt_busy = true;
            (ws.session_id.clone(), ws.pending_prompts.remove(0))
        };
        self.status("thinking");
        self.set_fork_status(key, "running", "Thinking").await;
        let result = self
            .send_wait(
                "session/prompt",
                json!({ "sessionId": session_id, "prompt": [{ "type": "text", "text": prompt }] }),
                Some(key.to_string()),
            )
            .await;
        let mut fork_completion = None;
        {
            let mut bridge = self.inner.state.lock().await;
            if let Some(ws) = bridge.workspaces.get_mut(key) {
                ws.prompt_busy = false;
                if let Some(fork) = ws.fork.as_mut()
                    && fork.status == "running"
                {
                    match &result {
                        Ok(_) => {
                            fork.status = "done".to_string();
                            fork.status_message = "Done".to_string();
                            let prompt = fork.prompt.clone();
                            let id = fork.id.clone();
                            fork_completion = Some((
                                "done".to_string(),
                                "Fork cursor complete".to_string(),
                                prompt,
                                id,
                            ));
                        }
                        Err(err) => {
                            fork.status = "error".to_string();
                            fork.status_message = compact_status(err);
                            let id = fork.id.clone();
                            fork_completion = Some((
                                "error".to_string(),
                                "Fork cursor failed".to_string(),
                                err.clone(),
                                id,
                            ));
                        }
                    }
                }
            }
        }
        if let Err(err) = result {
            self.ui_tx(UiEvent::Event {
                kind: "error".into(),
                title: "session/prompt".into(),
                body: err,
            });
        }
        if let Some((status, title, body, fork_id)) = fork_completion {
            self.emit_fork_sessions().await;
            self.ui_tx(UiEvent::ForkComplete {
                status,
                title,
                body,
                fork_id,
            });
        }
        self.status("ready");
        let has_more = {
            let bridge = self.inner.state.lock().await;
            bridge
                .workspaces
                .get(key)
                .is_some_and(|ws| !ws.pending_prompts.is_empty())
        };
        if has_more {
            Box::pin(self.flush_prompts(key)).await;
        }
    }

    async fn handle_agent_message(&self, message: Value) {
        if message.get("id").is_some()
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            self.handle_response(message).await;
            return;
        }
        if message.get("method").and_then(Value::as_str) == Some("session/update") {
            self.handle_session_update(message).await;
            return;
        }
        if message.get("id").is_some() {
            self.handle_client_request(message).await;
        }
    }

    async fn handle_response(&self, message: Value) {
        let id = message.get("id").and_then(Value::as_u64).unwrap_or(0);
        let pending = self.inner.pending.lock().await.remove(&id);
        let Some(mut pending) = pending else {
            return;
        };
        let result = if let Some(error) = message.get("error") {
            Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("ACP error")
                .to_string())
        } else {
            Ok(message.get("result").cloned().unwrap_or(Value::Null))
        };
        if pending.tx.is_some() {
            let _ = pending.tx.take().unwrap().send(result);
            return;
        }
        if let Err(err) = result {
            if let Some(key) = pending.workspace_key {
                self.append_entry(&key, "error", &pending.method, &err, Value::Null)
                    .await;
            }
        }
    }

    async fn handle_session_update(&self, message: Value) {
        let params = message.get("params").unwrap_or(&Value::Null);
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let key = {
            let bridge = self.inner.state.lock().await;
            bridge
                .sessions
                .get(session_id)
                .cloned()
                .unwrap_or_else(|| bridge.active_key.clone())
        };
        let update = params.get("update").unwrap_or(&Value::Null);
        match update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "agent_message_chunk" => {
                let body = text_from_content(update.get("content").unwrap_or(&Value::Null));
                if !body.trim().is_empty() {
                    self.append_or_merge(&key, "assistant", "Agent", &body)
                        .await;
                }
            }
            "agent_thought_chunk" => {
                let body = text_from_content(update.get("content").unwrap_or(&Value::Null));
                if !body.trim().is_empty() {
                    self.append_or_merge(&key, "thinking", "Thinking", &body)
                        .await;
                }
            }
            "tool_call" | "tool_call_update" => {
                let title = tool_title(update);
                let tool_name = tool_name_from_update(update);
                self.set_fork_status(
                    &key,
                    "running",
                    &tool_status_message(if tool_name.is_empty() {
                        title
                    } else {
                        &tool_name
                    }),
                )
                .await;
                self.upsert_tool_entry(&key, update).await;
            }
            "available_commands_update" => {
                let commands = update
                    .get("availableCommands")
                    .or_else(|| update.get("available_commands"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|value| serde_json::from_value(value).ok())
                    .collect::<Vec<AgentCommand>>();
                let mut bridge = self.inner.state.lock().await;
                if let Some(ws) = bridge.workspaces.get_mut(&key) {
                    ws.available_commands = commands;
                }
                drop(bridge);
                self.emit_workspace().await;
            }
            _ => {}
        }
    }

    async fn handle_client_request(&self, message: Value) {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let response = match method {
            "session/request_permission" => json!({
                "outcome": { "outcome": "selected", "optionId": strongest_allow(&params) }
            }),
            "fs/read_text_file" | "fs/readTextFile" | "read_text_file" | "readTextFile" => {
                match self.read_text_file(&params).await {
                    Ok(value) => value,
                    Err(err) => {
                        self.respond_error(id, -32000, err.to_string()).await;
                        return;
                    }
                }
            }
            "fs/write_text_file" | "fs/writeTextFile" | "write_text_file" | "writeTextFile" => {
                match self.write_text_file(&params).await {
                    Ok(value) => value,
                    Err(err) => {
                        self.respond_error(id, -32000, err.to_string()).await;
                        return;
                    }
                }
            }
            _ => {
                self.respond_error(id, -32601, format!("Unsupported client method: {method}"))
                    .await;
                return;
            }
        };
        self.respond(id, response).await;
    }

    async fn read_text_file(&self, params: &Value) -> Result<Value> {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing path"))?;
        let abs = self.resolve_workspace_path(path).await?;
        let content = tokio::fs::read_to_string(abs).await?;
        Ok(json!({ "content": content }))
    }

    async fn write_text_file(&self, params: &Value) -> Result<Value> {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing path"))?;
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing content"))?;
        let abs = self.resolve_workspace_path(path).await?;
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(abs, content).await?;
        Ok(json!({}))
    }

    async fn resolve_workspace_path(&self, requested: &str) -> Result<PathBuf> {
        let root = {
            let bridge = self.inner.state.lock().await;
            bridge
                .workspaces
                .get(&bridge.active_key)
                .map(|ws| ws.root.clone())
                .unwrap_or_else(|| {
                    self.inner
                        .config
                        .daemon
                        .workdir_root
                        .join("workspace-default")
                })
        };
        let abs = root
            .join(requested)
            .canonicalize()
            .unwrap_or_else(|_| root.join(requested));
        let rel = pathdiff::diff_paths(&abs, &root).ok_or_else(|| anyhow!("invalid path"))?;
        if rel.starts_with("..") || rel.is_absolute() {
            return Err(anyhow!("Path is outside TIC_CODEX_WORKDIR"));
        }
        Ok(abs)
    }

    async fn send_wait(
        &self,
        method: &str,
        params: Value,
        workspace_key: Option<String>,
    ) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        let id = self
            .send_request(method, params, workspace_key, Some(tx))
            .await
            .map_err(|e| e.to_string())?;
        rx.await
            .map_err(|_| format!("{method} response channel closed for {id}"))?
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let mut stdin = self.inner.stdin.lock().await;
        stdin
            .write_all(
                format!(
                    "{}\n",
                    json!({ "jsonrpc": "2.0", "method": method, "params": params })
                )
                .as_bytes(),
            )
            .await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn send_request(
        &self,
        method: &str,
        params: Value,
        workspace_key: Option<String>,
        tx: Option<oneshot::Sender<Result<Value, String>>>,
    ) -> Result<u64> {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        self.inner.pending.lock().await.insert(
            id,
            Pending {
                method: method.to_string(),
                workspace_key,
                tx,
            },
        );
        let mut stdin = self.inner.stdin.lock().await;
        stdin
            .write_all(
                format!(
                    "{}\n",
                    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
                )
                .as_bytes(),
            )
            .await?;
        stdin.flush().await?;
        Ok(id)
    }

    async fn respond(&self, id: Value, result: Value) {
        let mut stdin = self.inner.stdin.lock().await;
        let _ = stdin
            .write_all(
                format!(
                    "{}\n",
                    json!({ "jsonrpc": "2.0", "id": id, "result": result })
                )
                .as_bytes(),
            )
            .await;
    }

    async fn respond_error(&self, id: Value, code: i64, message: String) {
        let mut stdin = self.inner.stdin.lock().await;
        let _ = stdin
            .write_all(format!("{}\n", json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })).as_bytes())
            .await;
    }

    async fn append_entry(&self, key: &str, kind: &str, title: &str, body: &str, metadata: Value) {
        let mut bridge = self.inner.state.lock().await;
        let id = next_entry_id(&mut bridge);
        let ws = ensure_workspace(&self.inner.config, &mut bridge, key, key);
        ws.entries.push(AgentEntry {
            id,
            kind: kind.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            time: now_time(),
            metadata,
        });
        drop(bridge);
        self.snapshot(key).await;
    }

    async fn upsert_tool_entry(&self, key: &str, update: &Value) {
        let mut bridge = self.inner.state.lock().await;
        upsert_tool_entry_in_state(&self.inner.config, &mut bridge, key, update);
        drop(bridge);
        self.snapshot(key).await;
    }

    async fn append_or_merge(&self, key: &str, kind: &str, title: &str, body: &str) {
        let mut bridge = self.inner.state.lock().await;
        let id = next_entry_id(&mut bridge);
        let ws = ensure_workspace(&self.inner.config, &mut bridge, key, key);
        if let Some(last) = ws.entries.last_mut()
            && last.kind == kind
            && last.title == title
        {
            last.body.push_str(body);
            drop(bridge);
            self.snapshot(key).await;
            return;
        }
        ws.entries.push(AgentEntry {
            id,
            kind: kind.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            time: now_time(),
            metadata: Value::Null,
        });
        drop(bridge);
        self.snapshot(key).await;
    }

    async fn set_fork_status(&self, key: &str, status: &str, message: &str) {
        let changed = {
            let mut bridge = self.inner.state.lock().await;
            bridge
                .workspaces
                .get_mut(key)
                .and_then(|ws| ws.fork.as_mut())
                .map(|fork| {
                    fork.status = status.to_string();
                    fork.status_message = compact_status(message);
                })
                .is_some()
        };
        if changed {
            self.emit_fork_sessions().await;
        }
    }

    async fn snapshot(&self, key: &str) {
        let (active, events) = {
            let bridge = self.inner.state.lock().await;
            let active = bridge.active_key == key;
            let events = bridge
                .workspaces
                .get(key)
                .map(|ws| ws.entries.clone())
                .unwrap_or_default();
            (active, events)
        };
        if active {
            self.ui_tx(UiEvent::Snapshot { events });
            self.emit_workspace().await;
        }
    }

    async fn emit_workspace(&self) {
        let (key, title, commands) = {
            let bridge = self.inner.state.lock().await;
            let ws = bridge.workspaces.get(&bridge.active_key);
            (
                bridge.active_key.clone(),
                bridge.active_title.clone(),
                ws.map(command_list).unwrap_or_else(default_commands),
            )
        };
        self.ui_tx(UiEvent::Workspace {
            key,
            title,
            commands,
        });
    }

    async fn emit_fork_sessions(&self) {
        let sessions = {
            let bridge = self.inner.state.lock().await;
            bridge
                .fork_sessions
                .values()
                .filter_map(|key| {
                    let ws = bridge.workspaces.get(key)?;
                    let fork = ws.fork.as_ref()?;
                    Some(ForkSessionSummary {
                        id: fork.id.clone(),
                        key: ws.key.clone(),
                        title: fork.prompt.clone(),
                        status: fork.status.clone(),
                        status_message: fork.status_message.clone(),
                        cursor_theme: fork.cursor_theme.clone(),
                        cursor_id: fork.cursor_id.clone(),
                        session_id: fork.cua_session_id.clone(),
                        window_id: fork.window_id,
                        selected: ws.key == bridge.active_key,
                    })
                })
                .collect::<Vec<_>>()
        };
        self.ui_tx(UiEvent::ForkSessions { sessions });
    }

    fn status(&self, status: &str) {
        self.ui_tx(UiEvent::Status {
            status: status.to_string(),
        });
    }

    fn ui_tx(&self, event: UiEvent) {
        let _ = self.inner.ui_tx.send(event);
    }
}

fn ensure_workspace<'a>(
    config: &Config,
    bridge: &'a mut BridgeState,
    key: &str,
    title: &str,
) -> &'a mut WorkspaceState {
    bridge.workspaces.entry(key.to_string()).or_insert_with(|| {
        let workspace_id = workspace_id_from_key(key);
        WorkspaceState {
            key: key.to_string(),
            title: title.to_string(),
            workspace_id: workspace_id.clone(),
            root: config
                .daemon
                .workdir_root
                .join(safe_workspace_name(key, title)),
            ..WorkspaceState::default()
        }
    })
}

fn next_entry_id(bridge: &mut BridgeState) -> String {
    bridge.next_entry_id += 1;
    format!("entry:{}", bridge.next_entry_id)
}

fn upsert_tool_entry_in_state(
    config: &Config,
    bridge: &mut BridgeState,
    key: &str,
    update: &Value,
) {
    let call_id = tool_call_id(update);
    let merged = {
        let ws = ensure_workspace(config, bridge, key, key);
        if let Some(call_id) = call_id.as_deref() {
            let next = merge_tool_update(ws.tools.get(call_id), update);
            ws.tools.insert(call_id.to_string(), next.clone());
            next
        } else {
            update.clone()
        }
    };
    let title = tool_title(&merged).to_string();
    let body = content_from_tool(&merged);
    let metadata = tool_metadata(&merged);
    let entry_id = call_id
        .as_deref()
        .map(|id| format!("tool:{id}"))
        .unwrap_or_else(|| next_entry_id(bridge));
    let ws = ensure_workspace(config, bridge, key, key);
    if let Some(entry) = ws.entries.iter_mut().find(|entry| entry.id == entry_id) {
        entry.kind = "tool".to_string();
        entry.title = title;
        entry.body = body;
        entry.metadata = metadata;
    } else {
        ws.entries.push(AgentEntry {
            id: entry_id,
            kind: "tool".to_string(),
            title,
            body,
            time: now_time(),
            metadata,
        });
    }
}

fn sanitize_fork_id(id: Option<&str>) -> Option<String> {
    let id = id?.trim();
    if id.is_empty() {
        return None;
    }
    let normalized = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    Some(if normalized.starts_with("fork-") {
        normalized
    } else {
        format!("fork-{normalized}")
    })
}

fn workspace_id_from_key(key: &str) -> Option<String> {
    let marker = "workspace:";
    key.rsplit_once(marker)
        .map(|(_, id)| id)
        .or_else(|| key.strip_prefix(marker))
        .filter(|id| id.chars().all(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

fn safe_workspace_name(key: &str, title: &str) -> String {
    if let Some(id) = workspace_id_from_key(key) {
        return format!("workspace-{id}");
    }
    let raw = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if raw.is_empty() {
        "workspace-default".to_string()
    } else {
        raw
    }
}

fn compact_status(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() > 56 {
        format!("{}...", chars.iter().take(55).collect::<String>())
    } else {
        text
    }
}

fn tool_status_message(title: &str) -> String {
    match title.to_ascii_lowercase().as_str() {
        "click" => "Clicking".to_string(),
        "scroll" => "Scrolling".to_string(),
        "type-text" => "Typing".to_string(),
        "press-key" => "Pressing key".to_string(),
        "view-window" | "describe-workspace" => "Looking".to_string(),
        "close-session" => "Closing cursor".to_string(),
        _ => tool_status_message_from_title(title),
    }
}

fn tool_status_message_from_title(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    if lower.contains("click") {
        "Clicking".to_string()
    } else if lower.contains("scroll") {
        "Scrolling".to_string()
    } else if lower.contains("type-text") || lower.contains("type text") {
        "Typing".to_string()
    } else if lower.contains("press-key") || lower.contains("press key") {
        "Pressing key".to_string()
    } else if lower.contains("view-window") || lower.contains("screenshot") {
        "Looking".to_string()
    } else {
        compact_status(title)
    }
}

fn fork_workspace_title(prompt: &str) -> String {
    let trimmed = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = trimmed.chars().collect::<Vec<_>>();
    if chars.len() > 36 {
        format!("{}...", chars.iter().take(35).collect::<String>())
    } else if trimmed.is_empty() {
        "Fork cursor".to_string()
    } else {
        trimmed
    }
}

fn choose_cursor_theme() -> String {
    let mut themes = crate::niri::list_cursor_themes()
        .into_iter()
        .filter(|theme| theme != "Tiri-CUA")
        .collect::<Vec<_>>();
    if themes.is_empty() {
        themes.push("Tiri-CUA".to_string());
    }
    let len = themes.len();
    if len == 0 {
        return "Tiri-CUA".to_string();
    }
    let index = chrono::Utc::now().timestamp_millis().rem_euclid(len as i64) as usize;
    themes
        .get(index)
        .cloned()
        .unwrap_or_else(|| "Tiri-CUA".to_string())
}

fn fork_window_id(active_window: &Value) -> Option<u64> {
    active_window
        .get("id")
        .and_then(Value::as_u64)
        .or_else(|| active_window.get("windowId").and_then(Value::as_u64))
}

fn resolve_fork_active_window(active_window: Option<Value>) -> Result<Value> {
    if let Some(active_window) = active_window {
        if fork_window_id(&active_window).unwrap_or(0) != 0 {
            return Ok(active_window);
        }
    }

    let niri = crate::niri::Niri::discover()?;
    if let Ok(window) = niri.focused_window() {
        return serde_json::to_value(window).context("serialize focused niri window");
    }

    let workspaces = niri.workspaces()?;
    let windows = niri.windows()?;
    let active_window_id = workspaces
        .iter()
        .find(|workspace| workspace.is_focused)
        .and_then(|workspace| workspace.active_window_id)
        .or_else(|| {
            workspaces
                .iter()
                .find(|workspace| workspace.is_active)
                .and_then(|workspace| workspace.active_window_id)
        });
    if let Some(active_window_id) = active_window_id {
        if let Some(window) = windows
            .into_iter()
            .find(|window| window.id == active_window_id)
        {
            return serde_json::to_value(window).context("serialize active niri window");
        }
    }

    Err(anyhow!("no active window to fork cursor into"))
}

fn prepare_workspace_root(repo_root: &Path, ws: &WorkspaceState) -> Result<PathBuf> {
    fs::create_dir_all(&ws.root).with_context(|| format!("create {}", ws.root.display()))?;
    fs::write(ws.root.join("AGENTS.md"), agent_instructions(repo_root, ws))?;
    Ok(ws.root.clone())
}

fn session_new_params(key: &str, cwd: &Path, mcp_server: Value) -> Value {
    let mut params = json!({ "cwd": cwd, "mcpServers": [mcp_server] });
    if is_heart_workspace_key(key) {
        params["model"] = json!(HEARTBEAT_MODEL);
    }
    params
}

fn is_heart_workspace_key(key: &str) -> bool {
    key.starts_with("heart:window:") || key.starts_with("heart:workspace:")
}

fn agent_instructions(repo_root: &Path, ws: &WorkspaceState) -> String {
    let workspace_ref = ws
        .workspace_id
        .as_deref()
        .unwrap_or("(current focused workspace)");
    let heart = heart_instructions(ws);
    let fork = fork_instructions(ws);
    format!(
        "# Codex Workspace {}\n\nYou are running from a generated tic-shell ad-hoc workspace folder.\n\nCurrent niri workspace:\n- Key: {}\n- Title: {}\n- Numeric workspace id/index: {}\n{}\nThe `tic` MCP server is attached to this Codex session for computer-use actions.\nUse MCP tools exposed by that server. Do not run the legacy `cua ...` shell CLI unless an MCP tool is unavailable or fails.\nThe workspace `Key` is tic-shell UI metadata, not a CUA argument. Never pass values like `niri:workspace:1` as `workspace_id`; use the numeric id/index only, or omit `workspace_id` to use this session's workspace.\n\nUseful MCP tools:\n- `view-window` captures a single window. Prefer this when you already know the target window id.\n- `describe-workspace` returns window metadata and can include screenshots. Use it to discover window ids, and set `include_screenshots=true` when a visual workspace overview would help.\n- `click` clicks inside a window at window-relative screenshot/image pixel coordinates. It can return a `session_id`; pass the same `session_id` on later mouse calls for the same task so the painted virtual cursor is reused.\n- `type-text` types into a window.\n- `press-key` presses one named key in a window, for example `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, or `ArrowLeft`.\n- `scroll` scrolls in a window, with optional coordinates in window-relative screenshot/image pixels. Reuse the same `session_id` from prior mouse calls when continuing the same task.\n- `close-session` closes a mouse session and destroys its painted virtual cursor.\n- `set-window-description` sets or clears a very short live description shown for a window in the tic workspace sidebar and launcher. Only set one when the existing window title is not descriptive enough, and keep it easy to scan at a glance.\n- `set-workspace-name` sets a short human-readable name for a workspace.\n\nDo not call `describe-workspace` as a reflex before every action. If a recent tool result already identified the window, call `view-window` directly to inspect it; otherwise `include_screenshots=true` is a reasonable way to orient visually.\n\nPast activity summaries live under `~/.tic/memory/history/`. Look there when prior desktop activity would help answer the user's request or preserve continuity.\n{heart}\nThe real tic-shell checkout is at `{}` and is intentionally not your working directory. Use this folder for notes and temporary files, and use the `tic` MCP tools to inspect or act on the live desktop.\n",
        ws.title,
        ws.key,
        ws.title,
        workspace_ref,
        fork,
        repo_root.display()
    )
}

fn fork_instructions(ws: &WorkspaceState) -> String {
    let Some(fork) = ws.fork.as_ref() else {
        return String::new();
    };
    let title = fork
        .active_window
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    let app_id = fork
        .active_window
        .get("appId")
        .or_else(|| fork.active_window.get("app_id"))
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    format!(
        "\nForked cursor session:\n- Task: {}\n- Associated window id: {}\n- Associated window title: {}\n- Associated app id: {}\n- CUA session id: {}\n- Virtual cursor id: {}\n- Cursor theme: {}\n\nFor all mouse actions in this task, pass session_id \"{}\" when the tool accepts it. The same virtual cursor was pre-created at the user's cursor position, so do not start a second mouse session. When the task is complete, call close-session with session_id \"{}\".\n\n",
        fork.prompt,
        fork.window_id,
        title,
        app_id,
        fork.cua_session_id,
        fork.cursor_id,
        fork.cursor_theme,
        fork.cua_session_id,
        fork.cua_session_id,
    )
}

fn heart_instructions(ws: &WorkspaceState) -> &'static str {
    if ws.key.starts_with("heart:window:") {
        "\nHeart session note:\n- This is an L1 window heartbeat session, not normal user chat.\n- The user expects an initial L1 pass for each window the first time it is processed.\n- If the prompt says the current live window description is `(not set)`, inspect the supplied screenshot paths and call `set-window-description` early only when the existing window title is not descriptive enough.\n- Keep any window description very short so it is easy to scan at a glance.\n- If the current live window description is stale, inaccurate, or too verbose, update it with `set-window-description` before doing summary work.\n- After writing the requested summary and setting/updating the description if needed, call `emit_event` with event_type `window_heartbeat_l1_update`.\n\n"
    } else if ws.key.starts_with("heart:workspace:") {
        "\nHeart session note:\n- This is an L2 workspace heartbeat session, not normal user chat.\n- If the prompt says the workspace is unnamed and the activity is clear, call `set-workspace-name` with a short stable name before summary work.\n- After writing the requested summary and setting a workspace name if useful, call `emit_event` with event_type `workspace_heartbeat_l2_update`.\n\n"
    } else {
        ""
    }
}

fn adapter_command_spec(config: &Config) -> Result<(String, Vec<String>)> {
    if let Some(command) = config
        .daemon
        .adapter_command
        .as_ref()
        .filter(|s| !s.trim().is_empty())
    {
        return Ok((
            "bash".to_string(),
            vec!["-c".to_string(), format!("exec {command}")],
        ));
    }
    let bin = managed_adapter_bin(&config.daemon.data_root);
    if !bin.exists() {
        install_managed_adapter(&config.daemon.data_root)?;
    }
    Ok((bin.display().to_string(), Vec::new()))
}

fn managed_adapter_bin(data_root: &Path) -> PathBuf {
    data_root
        .join("codex-acp")
        .join("node_modules")
        .join(".bin")
        .join("codex-acp")
}

fn install_managed_adapter(data_root: &Path) -> Result<()> {
    let root = data_root.join("codex-acp");
    fs::create_dir_all(&root)?;
    let output = StdCommand::new("bun")
        .args([
            "install",
            "--cwd",
            root.to_str().unwrap_or(""),
            "--silent",
            "--no-progress",
            "--production",
            "--exact",
            "@zed-industries/codex-acp@0.13.0",
        ])
        .output()
        .context("install codex-acp")?;
    if !output.status.success() {
        return Err(anyhow!(
            "installing codex-acp failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn bridge_path() -> String {
    [
        "/run/current-system/sw/bin".to_string(),
        format!(
            "{}/.local/bin",
            std::env::var("HOME").unwrap_or_else(|_| "/home/jettc".into())
        ),
        format!(
            "{}/.cargo/bin",
            std::env::var("HOME").unwrap_or_else(|_| "/home/jettc".into())
        ),
        format!(
            "{}/.bun/bin",
            std::env::var("HOME").unwrap_or_else(|_| "/home/jettc".into())
        ),
        std::env::var("PATH").unwrap_or_default(),
    ]
    .join(":")
}

fn now_time() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

fn text_from_content(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(text_from_content)
            .collect::<Vec<_>>()
            .join(""),
        Value::Object(map) => map
            .get("text")
            .or_else(|| map.get("title"))
            .or_else(|| map.get("content"))
            .map(text_from_content)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn tool_call_id(update: &Value) -> Option<String> {
    update
        .get("toolCallId")
        .or_else(|| update.get("tool_call_id"))
        .or_else(|| update.get("id"))
        .and_then(|value| match value {
            Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
}

fn merge_tool_update(previous: Option<&Value>, update: &Value) -> Value {
    let Some(Value::Object(previous)) = previous else {
        return update.clone();
    };
    let Value::Object(update) = update else {
        return Value::Object(previous.clone());
    };
    let mut merged = previous.clone();
    for (key, value) in update {
        if key == "fields" {
            if let (Some(Value::Object(previous_fields)), Value::Object(update_fields)) =
                (merged.get("fields"), value)
            {
                let mut fields = previous_fields.clone();
                fields.extend(update_fields.clone());
                merged.insert(key.clone(), Value::Object(fields));
                continue;
            }
        }
        merged.insert(key.clone(), value.clone());
    }
    Value::Object(merged)
}

fn tool_title(update: &Value) -> &str {
    update
        .get("title")
        .or_else(|| update.pointer("/fields/title"))
        .and_then(Value::as_str)
        .unwrap_or("Tool call")
}

fn status_from_tool(update: &Value) -> &str {
    update
        .pointer("/fields/status")
        .or_else(|| update.get("status"))
        .or_else(|| update.pointer("/content/status"))
        .and_then(Value::as_str)
        .unwrap_or("pending")
}

fn content_from_tool(update: &Value) -> String {
    let mut parts = Vec::new();
    let status = status_from_tool(update);
    if !status.trim().is_empty() {
        parts.push(status.trim().to_string());
    }

    let content_text = text_from_content(
        update
            .get("content")
            .or_else(|| update.pointer("/fields/content"))
            .unwrap_or(&Value::Null),
    );
    if !content_text.trim().is_empty() {
        parts.push(content_text.trim().to_string());
    }

    let raw_output = update
        .get("rawOutput")
        .or_else(|| update.get("raw_output"))
        .or_else(|| update.pointer("/fields/rawOutput"))
        .or_else(|| update.pointer("/fields/raw_output"))
        .unwrap_or(&Value::Null);
    let raw_output_text = text_from_content(raw_output);
    if !raw_output_text.trim().is_empty() {
        parts.push(raw_output_text.trim().to_string());
    }

    parts.join("\n")
}

fn tool_content(update: &Value) -> &Value {
    update
        .get("content")
        .or_else(|| update.pointer("/fields/content"))
        .or_else(|| update.get("rawOutput"))
        .or_else(|| update.get("raw_output"))
        .or_else(|| update.pointer("/fields/rawOutput"))
        .or_else(|| update.pointer("/fields/raw_output"))
        .unwrap_or(&Value::Null)
}

fn image_metadata_from_content(value: &Value) -> Option<Value> {
    let image = find_image_content(value)?;
    let map = image.as_object()?;
    let data = map
        .get("data")
        .or_else(|| map.get("base64"))
        .or_else(|| map.get("source"))
        .and_then(Value::as_str)?;
    if data.trim().is_empty() {
        return None;
    }
    let mime_type = map
        .get("mimeType")
        .or_else(|| map.get("mime_type"))
        .or_else(|| map.get("mediaType"))
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    let source = if data.starts_with("data:") || data.starts_with("file:") {
        data.to_string()
    } else {
        format!("data:{mime_type};base64,{data}")
    };
    Some(json!({
        "source": source,
        "mimeType": mime_type,
    }))
}

fn find_image_content(value: &Value) -> Option<&Value> {
    match value {
        Value::Array(items) => items.iter().find_map(find_image_content),
        Value::Object(map) => {
            let content_type = map
                .get("type")
                .or_else(|| map.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if content_type == "image"
                || content_type == "input_image"
                || map
                    .get("mimeType")
                    .or_else(|| map.get("mime_type"))
                    .and_then(Value::as_str)
                    .is_some_and(|mime| mime.starts_with("image/"))
            {
                return Some(value);
            }
            map.get("content")
                .or_else(|| map.get("value"))
                .or_else(|| map.get("result"))
                .and_then(find_image_content)
        }
        _ => None,
    }
}

fn tool_metadata(update: &Value) -> Value {
    let tool_name = tool_name_from_update(update);
    let mut metadata = json!({ "toolName": tool_name, "isCua": !tool_name.is_empty() });
    if let Some(image) = image_metadata_from_content(tool_content(update))
        && let Some(map) = metadata.as_object_mut()
    {
        map.insert("image".to_string(), image);
    }
    metadata
}

fn tool_name_from_update(update: &Value) -> String {
    let mut candidates = Vec::new();
    collect_tool_name_candidates(update, &mut candidates);
    let haystack = candidates.join(" ").to_ascii_lowercase();
    let tool_name = [
        "describe-workspace",
        "view-window",
        "click",
        "type-text",
        "press-key",
        "scroll",
        "close-session",
        "emit_event",
        "set-window-description",
        "set-workspace-name",
    ]
    .into_iter()
    .find(|name| haystack.contains(name))
    .unwrap_or("");
    tool_name.to_string()
}

fn collect_tool_name_candidates(value: &Value, candidates: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_tool_name_candidates(item, candidates);
            }
        }
        Value::Object(map) => {
            for key in ["title", "name", "toolName", "tool_name", "method"] {
                if let Some(text) = map.get(key).and_then(Value::as_str) {
                    candidates.push(text.to_string());
                }
            }
            for key in ["fields", "tool", "toolCall", "tool_call", "arguments"] {
                if let Some(next) = map.get(key) {
                    collect_tool_name_candidates(next, candidates);
                }
            }
        }
        _ => {}
    }
}

fn strongest_allow(params: &Value) -> String {
    params
        .get("options")
        .and_then(Value::as_array)
        .and_then(|options| {
            options
                .iter()
                .find(|item| item.get("kind").and_then(Value::as_str) == Some("allow_always"))
                .or_else(|| {
                    options
                        .iter()
                        .find(|item| item.get("kind").and_then(Value::as_str) == Some("allow_once"))
                })
                .or_else(|| options.first())
        })
        .and_then(|item| item.get("optionId").and_then(Value::as_str))
        .unwrap_or("allow")
        .to_string()
}

fn default_commands() -> Vec<AgentCommand> {
    vec![
        AgentCommand {
            name: "clear".into(),
            description: "Clear this workspace session".into(),
        },
        AgentCommand {
            name: "new".into(),
            description: "Start a new session for this workspace".into(),
        },
        AgentCommand {
            name: "cancel".into(),
            description: "Cancel the running turn".into(),
        },
        AgentCommand {
            name: "help".into(),
            description: "Show available slash commands".into(),
        },
    ]
}

fn command_list(ws: &WorkspaceState) -> Vec<AgentCommand> {
    let mut commands = default_commands();
    commands.extend(ws.available_commands.clone());
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_key_extracts_numeric_id() {
        assert_eq!(
            workspace_id_from_key("niri:workspace:2").as_deref(),
            Some("2")
        );
        assert_eq!(workspace_id_from_key("workspace:7").as_deref(), Some("7"));
    }

    #[test]
    fn normal_agent_instructions_do_not_use_heartbeat_language() {
        let ws = WorkspaceState {
            key: "niri:workspace:2".to_string(),
            title: "Workspace 2".to_string(),
            workspace_id: Some("2".to_string()),
            ..WorkspaceState::default()
        };

        let instructions = agent_instructions(Path::new("/repo"), &ws);

        assert!(
            instructions.contains("Past activity summaries live under `~/.tic/memory/history/`")
        );
        assert!(instructions.contains("The `tic` MCP server is attached"));
        assert!(!instructions.contains("L1 window heartbeat"));
        assert!(!instructions.contains("L2 workspace heartbeat"));
        assert!(!instructions.contains("window_heartbeat_l1_update"));
        assert!(!instructions.contains("workspace_heartbeat_l2_update"));
    }

    #[test]
    fn heart_agent_instructions_include_event_guidance() {
        let ws = WorkspaceState {
            key: "heart:workspace:6".to_string(),
            title: "L2 workspace 6".to_string(),
            workspace_id: Some("6".to_string()),
            ..WorkspaceState::default()
        };

        let instructions = agent_instructions(Path::new("/repo"), &ws);

        assert!(instructions.contains("This is an L2 workspace heartbeat session"));
        assert!(instructions.contains("set-workspace-name"));
        assert!(instructions.contains("emit_event"));
        assert!(instructions.contains("workspace_heartbeat_l2_update"));
    }

    #[test]
    fn normal_session_params_do_not_pin_model() {
        let params = session_new_params(
            "niri:workspace:2",
            Path::new("/tmp/tic-workspace"),
            json!({ "name": "tic" }),
        );

        assert_eq!(params.get("model"), None);
        assert_eq!(
            params
                .get("mcpServers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn heart_session_params_use_mini_model() {
        for key in ["heart:window:15", "heart:workspace:6"] {
            let params =
                session_new_params(key, Path::new("/tmp/tic-heart"), json!({ "name": "tic" }));

            assert_eq!(
                params.get("model").and_then(Value::as_str),
                Some(HEARTBEAT_MODEL)
            );
        }
    }

    #[test]
    fn extracts_mcp_image_content_for_tool_metadata() {
        let content = json!([
            { "type": "text", "text": "{\"path\":\"/tmp/window.png\"}" },
            { "type": "image", "data": "abc123", "mimeType": "image/png" }
        ]);

        let metadata = image_metadata_from_content(&content).unwrap();

        assert_eq!(
            metadata.pointer("/source").and_then(Value::as_str),
            Some("data:image/png;base64,abc123")
        );
        assert_eq!(
            metadata.pointer("/mimeType").and_then(Value::as_str),
            Some("image/png")
        );
    }

    #[test]
    fn tool_metadata_reads_nested_title() {
        let update = json!({
            "fields": {
                "title": "view-window"
            }
        });

        let metadata = tool_metadata(&update);

        assert_eq!(
            metadata.pointer("/toolName").and_then(Value::as_str),
            Some("view-window")
        );
        assert_eq!(
            metadata.pointer("/isCua").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn tool_metadata_reads_nested_tool_name() {
        let update = json!({
            "title": "Tool call",
            "fields": {
                "tool": {
                    "name": "click"
                }
            }
        });

        let metadata = tool_metadata(&update);

        assert_eq!(
            metadata.pointer("/toolName").and_then(Value::as_str),
            Some("click")
        );
        assert_eq!(tool_status_message("click"), "Clicking");
    }

    #[test]
    fn tool_updates_replace_existing_entry() {
        let config = Config::default();
        let mut bridge = BridgeState::default();

        upsert_tool_entry_in_state(
            &config,
            &mut bridge,
            "workspace:test",
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "Read file",
                "status": "pending"
            }),
        );
        upsert_tool_entry_in_state(
            &config,
            &mut bridge,
            "workspace:test",
            &json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "fields": {
                    "status": "completed",
                    "content": { "type": "text", "text": "done" }
                }
            }),
        );

        let entries = &bridge.workspaces["workspace:test"].entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "tool:tool-1");
        assert_eq!(entries[0].title, "Read file");
        assert_eq!(entries[0].body, "completed\ndone");
    }

    #[test]
    fn merged_tool_entry_keeps_image_metadata() {
        let config = Config::default();
        let mut bridge = BridgeState::default();

        upsert_tool_entry_in_state(
            &config,
            &mut bridge,
            "workspace:test",
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-image",
                "title": "view-window",
                "status": "pending"
            }),
        );
        upsert_tool_entry_in_state(
            &config,
            &mut bridge,
            "workspace:test",
            &json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-image",
                "fields": {
                    "status": "completed",
                    "content": [{ "type": "image", "mimeType": "image/png", "data": "iVBORw0KGgo=" }]
                }
            }),
        );

        let entry = &bridge.workspaces["workspace:test"].entries[0];
        assert_eq!(
            entry.metadata.pointer("/toolName").and_then(Value::as_str),
            Some("view-window")
        );
        assert_eq!(
            entry.metadata.pointer("/isCua").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            entry
                .metadata
                .pointer("/image/source")
                .and_then(Value::as_str),
            Some("data:image/png;base64,iVBORw0KGgo=")
        );
    }
}
