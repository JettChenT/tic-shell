use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    time::Instant,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
    acp::AcpHandle,
    config::{Config, parse_duration},
    niri::{Niri, TiriEvent, Window, Workspace, windows_by_id},
    ui::UiEvent,
};

#[derive(Debug, Clone)]
pub enum Event {
    TiriEvent(TiriEvent),
    Cron {
        name: String,
        fired_at: DateTime<Local>,
    },
    Screenshot {
        window_id: u64,
        screenshot_data: ScreenshotData,
    },
    HeartUpdate(HeartUpdate),
    WindowDescription {
        window_id: u64,
        description: String,
    },
    WorkspaceName {
        workspace_id: u64,
        name: String,
    },
}

impl Event {
    fn debug_label(&self) -> String {
        match self {
            Event::TiriEvent(event) => format!("tiri:{event:?}"),
            Event::Cron { name, .. } => format!("cron:{name}"),
            Event::Screenshot { window_id, .. } => format!("screenshot:{window_id}"),
            Event::HeartUpdate(update) => format!("heart-update:{}", update.event_type),
            Event::WindowDescription { window_id, .. } => {
                format!("window-description:{window_id}")
            }
            Event::WorkspaceName { workspace_id, .. } => {
                format!("workspace-name:{workspace_id}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScreenshotData {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct HeartUpdate {
    pub event_type: String,
    pub description: String,
    pub window_id: Option<u64>,
    pub workspace_id: Option<u64>,
}

#[derive(Debug)]
struct WindowHeartbeat {
    window: Window,
    buffer: VecDeque<ScreenshotData>,
    accepted_since_trigger: usize,
    initialized: bool,
    started: Instant,
    last_trigger: Instant,
    last_description_request: Option<Instant>,
    last_hash: Option<img_hash::ImageHash>,
    last_trigger_hash: Option<img_hash::ImageHash>,
}

#[derive(Debug)]
struct WorkspaceHeartbeat {
    workspace: Workspace,
    seen_first_windows: HashSet<u64>,
    pending_updates: Vec<HeartUpdate>,
    last_trigger: Instant,
}

pub struct HeartSupervisor {
    config: Config,
    acp: AcpHandle,
    ui_tx: mpsc::UnboundedSender<UiEvent>,
    niri: Niri,
    windows: HashMap<u64, WindowHeartbeat>,
    workspaces: HashMap<u64, WorkspaceHeartbeat>,
    window_descriptions: HashMap<u64, String>,
    workspace_names: HashMap<u64, String>,
    last_event: String,
    last_debug_sent: Instant,
}

impl HeartSupervisor {
    pub fn spawn(config: Config, acp: AcpHandle, ui_tx: mpsc::UnboundedSender<UiEvent>) {
        tokio::spawn(async move {
            if let Err(err) = Self::run(config, acp, ui_tx).await {
                eprintln!("tic-daemon heart stopped: {err:#}");
            }
        });
    }

    async fn run(
        config: Config,
        acp: AcpHandle,
        ui_tx: mpsc::UnboundedSender<UiEvent>,
    ) -> Result<()> {
        if !config.heartbeat.window_l1.enabled && !config.heartbeat.workspace_l2.enabled {
            return Ok(());
        }
        fs::create_dir_all(config.history_dir())
            .with_context(|| format!("create {}", config.history_dir().display()))?;
        let niri = Niri::discover()?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_event_stream(niri.clone(), tx.clone());
        spawn_screenshot_cron(config.clone(), niri.clone(), tx.clone());
        spawn_workspace_cron(config.clone(), tx.clone());
        spawn_event_log_tail(config.clone(), tx);
        let mut supervisor = Self {
            config,
            acp,
            ui_tx,
            niri,
            windows: HashMap::new(),
            workspaces: HashMap::new(),
            window_descriptions: HashMap::new(),
            workspace_names: HashMap::new(),
            last_event: "bootstrap".to_string(),
            last_debug_sent: Instant::now(),
        };
        supervisor.bootstrap().await?;
        while let Some(event) = rx.recv().await {
            supervisor.handle_event(event).await;
        }
        Ok(())
    }

    async fn bootstrap(&mut self) -> Result<()> {
        for workspace in self.niri.workspaces().unwrap_or_default() {
            self.ensure_workspace(workspace);
        }
        for window in self.niri.windows().unwrap_or_default() {
            self.ensure_window(window);
        }
        let _ = self.ui_tx.send(UiEvent::DebugSnapshot {
            snapshot: self.debug_snapshot(),
        });
        Ok(())
    }

    async fn handle_event(&mut self, event: Event) {
        self.last_event = event.debug_label();
        match event {
            Event::TiriEvent(event) => self.handle_tiri_event(event).await,
            Event::Cron { name, fired_at } if name == "window-l1-screenshot" => {
                let _ = fired_at;
            }
            Event::Cron { name, fired_at } if name == "workspace-l2" => {
                let _ = fired_at;
                self.trigger_all_workspaces("interval").await;
            }
            Event::Screenshot {
                window_id,
                screenshot_data,
            } => self.handle_screenshot(window_id, screenshot_data).await,
            Event::HeartUpdate(update) => self.handle_heart_update(update).await,
            Event::WindowDescription {
                window_id,
                description,
            } => {
                if description.trim().is_empty() {
                    self.window_descriptions.remove(&window_id);
                } else {
                    self.window_descriptions
                        .insert(window_id, description.clone());
                    if let Some(heartbeat) = self.windows.get_mut(&window_id) {
                        heartbeat.last_description_request = None;
                    }
                }
                let _ = self.ui_tx.send(UiEvent::WindowDescription {
                    window_id,
                    description,
                });
            }
            Event::WorkspaceName { workspace_id, name } => {
                if name.trim().is_empty() {
                    self.workspace_names.remove(&workspace_id);
                } else {
                    self.workspace_names.insert(workspace_id, name.clone());
                }
                let _ = self
                    .ui_tx
                    .send(UiEvent::WorkspaceName { workspace_id, name });
            }
            _ => {}
        }
        self.maybe_publish_debug_snapshot();
    }

    fn maybe_publish_debug_snapshot(&mut self) {
        if self.last_debug_sent.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }
        self.last_debug_sent = Instant::now();
        let _ = self.ui_tx.send(UiEvent::DebugSnapshot {
            snapshot: self.debug_snapshot(),
        });
    }

    fn debug_snapshot(&self) -> serde_json::Value {
        let windows_total = self.windows.len();
        let windows_initialized = self
            .windows
            .values()
            .filter(|heartbeat| heartbeat.initialized)
            .count();
        let buffered_screenshots = self
            .windows
            .values()
            .map(|heartbeat| heartbeat.buffer.len())
            .sum::<usize>();
        let pending_workspace_updates = self
            .workspaces
            .values()
            .map(|heartbeat| heartbeat.pending_updates.len())
            .sum::<usize>();
        json!({
            "time": Utc::now().to_rfc3339(),
            "heartbeat": {
                "enabled": {
                    "windowL1": self.config.heartbeat.window_l1.enabled,
                    "workspaceL2": self.config.heartbeat.workspace_l2.enabled,
                    "screenshotDiff": self.config.heartbeat.screenshot_diff.enabled
                },
                "windowsTotal": windows_total,
                "windowsInitialized": windows_initialized,
                "windowsPendingInitial": windows_total.saturating_sub(windows_initialized),
                "bufferedScreenshots": buffered_screenshots,
                "workspacesTotal": self.workspaces.len(),
                "pendingWorkspaceUpdates": pending_workspace_updates,
                "lastEvent": self.last_event
            },
            "paths": {
                "dataRoot": self.config.daemon.data_root,
                "historyDir": self.config.history_dir(),
                "events": self.config.daemon.data_root.join("events.jsonl"),
                "windowDescriptions": "(memory only)"
            }
        })
    }

    async fn handle_tiri_event(&mut self, event: TiriEvent) {
        match event {
            TiriEvent::WorkspacesChanged { workspaces } => {
                let existing: HashSet<u64> = workspaces.iter().map(|ws| ws.id).collect();
                let closed = self
                    .workspaces
                    .keys()
                    .copied()
                    .filter(|id| !existing.contains(id))
                    .collect::<Vec<_>>();
                for workspace in workspaces {
                    self.ensure_workspace(workspace);
                }
                for id in closed {
                    self.close_workspace(id).await;
                }
            }
            TiriEvent::WindowsChanged { windows } => {
                let existing = windows_by_id(&windows);
                let closed = self
                    .windows
                    .keys()
                    .copied()
                    .filter(|id| !existing.contains_key(id))
                    .collect::<Vec<_>>();
                for window in windows {
                    self.ensure_window(window);
                }
                for id in closed {
                    self.close_window(id).await;
                }
            }
            TiriEvent::WindowOpenedOrChanged { window } => self.ensure_window(window),
            TiriEvent::WindowClosed { id } => self.close_window(id).await,
            TiriEvent::WindowLayoutsChanged { changes } => {
                for (id, layout) in changes {
                    if let Some(heartbeat) = self.windows.get_mut(&id) {
                        heartbeat.window.layout = layout;
                    }
                }
            }
            _ => {}
        }
    }

    fn ensure_window(&mut self, window: Window) {
        let window_id = window.id;
        self.windows
            .entry(window_id)
            .and_modify(|heartbeat| heartbeat.window = window.clone())
            .or_insert_with(|| WindowHeartbeat {
                window: window.clone(),
                buffer: VecDeque::new(),
                accepted_since_trigger: 0,
                initialized: false,
                started: Instant::now(),
                last_trigger: Instant::now(),
                last_description_request: None,
                last_hash: None,
                last_trigger_hash: None,
            });
    }

    fn ensure_workspace(&mut self, workspace: Workspace) {
        self.workspaces
            .entry(workspace.id)
            .and_modify(|heartbeat| heartbeat.workspace = workspace.clone())
            .or_insert_with(|| WorkspaceHeartbeat {
                workspace,
                seen_first_windows: HashSet::new(),
                pending_updates: Vec::new(),
                last_trigger: Instant::now(),
            });
    }

    async fn handle_screenshot(&mut self, window_id: u64, screenshot: ScreenshotData) {
        let description_missing = self
            .window_description(window_id)
            .is_none_or(|description| description.trim().is_empty());
        let Some(heartbeat) = self.windows.get_mut(&window_id) else {
            return;
        };
        let description_retry_ready = heartbeat.last_description_request.is_none_or(|requested| {
            requested.elapsed() >= parse_duration(&self.config.heartbeat.window_l1.ongoing.interval)
        });
        let needs_description_backfill =
            heartbeat.initialized && description_missing && description_retry_ready;
        if heartbeat.initialized
            && !needs_description_backfill
            && !significantly_different(&self.config, heartbeat, &screenshot)
        {
            let _ = fs::remove_file(&screenshot.path);
            return;
        }
        heartbeat.last_hash = image_hash(&screenshot.path).ok();
        heartbeat.buffer.push_back(screenshot);
        heartbeat.accepted_since_trigger += 1;
        while heartbeat.buffer.len() > self.config.heartbeat.window_l1.buffer_max {
            if let Some(old) = heartbeat.buffer.pop_front() {
                let _ = fs::remove_file(old.path);
            }
        }
        let now = Instant::now();
        let initial_ready = !heartbeat.initialized
            && (heartbeat.accepted_since_trigger
                >= self.config.heartbeat.window_l1.initial_screenshot_count
                || heartbeat.started.elapsed()
                    >= parse_duration(&self.config.heartbeat.window_l1.initial_timeout));
        let changed_since_trigger = changed_since_last_trigger(&self.config, heartbeat);
        let ongoing_ready = heartbeat.initialized
            && changed_since_trigger
            && (heartbeat.accepted_since_trigger
                >= self.config.heartbeat.window_l1.ongoing_screenshot_count
                || heartbeat.last_trigger.elapsed()
                    >= parse_duration(&self.config.heartbeat.window_l1.ongoing.interval));
        if initial_ready || ongoing_ready || needs_description_backfill {
            heartbeat.initialized = true;
            heartbeat.accepted_since_trigger = 0;
            heartbeat.last_trigger = now;
            heartbeat.last_trigger_hash = heartbeat.last_hash.clone();
            if needs_description_backfill {
                heartbeat.last_description_request = Some(now);
            }
            let reason = if needs_description_backfill {
                "description missing"
            } else {
                "interval"
            };
            self.trigger_window(window_id, reason).await;
        }
    }

    async fn close_window(&mut self, window_id: u64) {
        if self.windows.contains_key(&window_id) {
            self.trigger_window(window_id, "window closed").await;
            self.windows.remove(&window_id);
        }
    }

    async fn trigger_window(&mut self, window_id: u64, reason: &str) {
        let Some(heartbeat) = self.windows.get_mut(&window_id) else {
            return;
        };
        let screenshots = heartbeat.buffer.drain(..).collect::<Vec<_>>();
        if screenshots.is_empty() && reason != "window closed" {
            return;
        }
        let workspace_id = heartbeat.window.workspace_id;
        let title = heartbeat
            .window
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".into());
        let summary_path = summary_path(
            &self.config,
            &self.config.heartbeat.window_l1.summary_duration_label,
            "window-summary",
        );
        let current_description = self
            .window_description(window_id)
            .filter(|description| !description.trim().is_empty())
            .unwrap_or_else(|| "(not set)".to_string());
        let screenshot_list = screenshots
            .iter()
            .map(|shot| format!("- {}", shot.path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are the L1 window heartbeat for niri window {window_id} ({title}).\nReason: {reason}.\nCurrent live window description: {current_description}\n\nScreenshots to consume:\n{screenshot_list}\n\nFirst inspect the screenshots enough to identify the current activity. If the current live window description is `(not set)`, only call MCP `set-window-description` when the existing window title is not descriptive enough. If you set it, use a very short present-tense phrase that is easy to scan at a glance, such as \"compiling Q1 finance stats\". If the title already makes the activity clear, leave the description unset. If the current description is stale, inaccurate, or too verbose, update it with the same tool before doing summary work.\n\nThen update or create this narrative summary file:\n{}\n\nDescribe what happened during this window interval. Look up past summaries under ~/.tic/memory/history/ for continuity. After writing the summary and setting/updating the description if needed, call MCP `emit_event` with event_type `window_heartbeat_l1_update`, window_id {window_id}, workspace_id {}, and a textual description of what happened.",
            summary_path.display(),
            workspace_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "null".into())
        );
        self.acp
            .prompt_heart(
                &format!("heart:window:{window_id}"),
                &format!("L1 window {window_id}"),
                prompt,
            )
            .await;
    }

    fn window_description(&self, window_id: u64) -> Option<String> {
        self.window_descriptions.get(&window_id).cloned()
    }

    async fn handle_heart_update(&mut self, update: HeartUpdate) {
        if update.event_type != "window_heartbeat_l1_update" {
            return;
        }
        let workspace_id = update.workspace_id.or_else(|| {
            update
                .window_id
                .and_then(|id| self.windows.get(&id))
                .and_then(|heartbeat| heartbeat.window.workspace_id)
        });
        let Some(workspace_id) = workspace_id else {
            return;
        };
        let Some(workspace) = self.workspaces.get_mut(&workspace_id) else {
            return;
        };
        let first = update
            .window_id
            .is_some_and(|id| workspace.seen_first_windows.insert(id));
        workspace.pending_updates.push(update);
        if first {
            self.trigger_workspace(workspace_id, "first window update")
                .await;
        }
    }

    async fn trigger_all_workspaces(&mut self, reason: &str) {
        let ids = self.workspaces.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.trigger_workspace(id, reason).await;
        }
    }

    async fn close_workspace(&mut self, workspace_id: u64) {
        self.trigger_workspace(workspace_id, "workspace closed")
            .await;
        self.workspaces.remove(&workspace_id);
    }

    async fn trigger_workspace(&mut self, workspace_id: u64, reason: &str) {
        let stored_name = self.workspace_names.get(&workspace_id).cloned();
        let Some(heartbeat) = self.workspaces.get_mut(&workspace_id) else {
            return;
        };
        if heartbeat.pending_updates.is_empty()
            && reason != "workspace closed"
            && reason != "interval"
        {
            return;
        }
        let updates = std::mem::take(&mut heartbeat.pending_updates);
        heartbeat.last_trigger = Instant::now();
        let current_name = stored_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                heartbeat
                    .workspace
                    .name
                    .as_deref()
                    .filter(|name| !name.trim().is_empty())
            })
            .unwrap_or("(unnamed)");
        let summary_path = summary_path(
            &self.config,
            &self.config.heartbeat.workspace_l2.summary_duration_label,
            "workspace-summary",
        );
        let update_text = updates
            .iter()
            .map(|update| format!("- window {:?}: {}", update.window_id, update.description))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are the L2 workspace heartbeat for niri workspace {workspace_id}.\nReason: {reason}.\nCurrent workspace name: {current_name}\n\nWindow heartbeat updates for this duration:\n{update_text}\n\nIf the current workspace name is `(unnamed)` and the workspace activity is clear, call MCP `set-workspace-name` with workspace_id {workspace_id} and a short name before writing the summary. Keep the name stable, human-readable, and easy to scan.\n\nUpdate or create this narrative summary file:\n{}\n\nLook up past summaries under ~/.tic/memory/history/ for continuity. After writing the summary and setting a workspace name if useful, call MCP `emit_event` with event_type `workspace_heartbeat_l2_update`, workspace_id {workspace_id}, and a textual description of what happened.",
            summary_path.display()
        );
        self.acp
            .prompt_heart(
                &format!("heart:workspace:{workspace_id}"),
                &format!("L2 workspace {workspace_id}"),
                prompt,
            )
            .await;
    }
}

fn spawn_event_stream(niri: Niri, tx: mpsc::UnboundedSender<Event>) {
    tokio::task::spawn_blocking(move || {
        let Ok(mut reader) = niri.event_stream() else {
            return;
        };
        while let Ok(event) = reader.read_event() {
            let _ = tx.send(Event::TiriEvent(event));
        }
    });
}

fn spawn_screenshot_cron(config: Config, niri: Niri, tx: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(parse_duration(
            &config.heartbeat.window_l1.screenshot_interval,
        ));
        loop {
            ticker.tick().await;
            let _ = tx.send(Event::Cron {
                name: "window-l1-screenshot".into(),
                fired_at: Local::now(),
            });
            for window in niri.windows().unwrap_or_default() {
                let path = config.daemon.data_root.join("screenshots").join(format!(
                    "window-{}-{}.png",
                    window.id,
                    chrono::Utc::now().timestamp_millis()
                ));
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if niri.screenshot_window(window.id, &path).is_ok() {
                    let _ = tx.send(Event::Screenshot {
                        window_id: window.id,
                        screenshot_data: ScreenshotData { path },
                    });
                }
            }
        }
    });
}

fn spawn_workspace_cron(config: Config, tx: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let mut ticker =
            tokio::time::interval(parse_duration(&config.heartbeat.workspace_l2.interval));
        loop {
            ticker.tick().await;
            let _ = tx.send(Event::Cron {
                name: "workspace-l2".into(),
                fired_at: Local::now(),
            });
        }
    });
}

fn spawn_event_log_tail(config: Config, tx: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let path = config.daemon.data_root.join("events.jsonl");
        let mut seen = fs::read_to_string(&path)
            .map(|content| content.lines().count())
            .unwrap_or(0);
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            ticker.tick().await;
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let lines = content.lines().collect::<Vec<_>>();
            let start = seen.min(lines.len());
            for line in lines.iter().skip(start) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    let event_type = value
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let description = value
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if event_type == "window_description_set" {
                        if let Some(window_id) =
                            value.get("window_id").and_then(serde_json::Value::as_u64)
                        {
                            let _ = tx.send(Event::WindowDescription {
                                window_id,
                                description,
                            });
                        }
                    } else if event_type == "workspace_name_set" {
                        if let Some(workspace_id) = value
                            .get("workspace_id")
                            .and_then(serde_json::Value::as_u64)
                        {
                            let name = value
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let _ = tx.send(Event::WorkspaceName { workspace_id, name });
                        }
                    } else if !event_type.is_empty() {
                        let _ = tx.send(Event::HeartUpdate(HeartUpdate {
                            event_type,
                            description,
                            window_id: value.get("window_id").and_then(serde_json::Value::as_u64),
                            workspace_id: value
                                .get("workspace_id")
                                .and_then(serde_json::Value::as_u64),
                        }));
                    }
                }
            }
            seen = lines.len();
        }
    });
}

fn image_hash(path: &PathBuf) -> Result<img_hash::ImageHash> {
    let image = img_hash::image::open(path).with_context(|| format!("open {}", path.display()))?;
    Ok(img_hash::HasherConfig::new().to_hasher().hash_image(&image))
}

fn significantly_different(
    config: &Config,
    heartbeat: &WindowHeartbeat,
    screenshot: &ScreenshotData,
) -> bool {
    if !config.heartbeat.screenshot_diff.enabled {
        return true;
    }
    let Some(previous) = heartbeat.last_hash.as_ref() else {
        return true;
    };
    let Ok(next) = image_hash(&screenshot.path) else {
        return true;
    };
    previous.dist(&next) >= config.heartbeat.screenshot_diff.threshold
}

fn changed_since_last_trigger(config: &Config, heartbeat: &WindowHeartbeat) -> bool {
    if !config.heartbeat.screenshot_diff.enabled {
        return heartbeat.accepted_since_trigger > 0;
    }
    match (
        heartbeat.last_trigger_hash.as_ref(),
        heartbeat.last_hash.as_ref(),
    ) {
        (Some(previous), Some(current)) => {
            previous.dist(current) >= config.heartbeat.screenshot_diff.threshold
        }
        (None, Some(_)) => heartbeat.accepted_since_trigger > 0,
        _ => false,
    }
}

fn summary_path(config: &Config, duration_label: &str, suffix: &str) -> PathBuf {
    let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
    config
        .history_dir()
        .join(format!("{timestamp}-{duration_label}-{suffix}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_enum_has_required_variants() {
        let _ = Event::Cron {
            name: "test".into(),
            fired_at: Local::now(),
        };
    }
}
