use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command as TokioCommand,
    sync::mpsc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceFocus {
    Inactive,
    Active,
    Focused,
}

impl WorkspaceFocus {
    pub fn is_current(&self) -> bool {
        matches!(self, Self::Active | Self::Focused)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NiriWorkspace {
    pub id: u64,
    pub key: String,
    pub idx: i64,
    pub name: String,
    pub label: String,
    pub output: String,
    pub focus: WorkspaceFocus,
    pub urgent: bool,
    pub active_window_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NiriWindow {
    pub id: u64,
    pub key: String,
    pub title: String,
    pub app_id: String,
    pub workspace_id: Option<u64>,
    pub focused: bool,
    pub floating: bool,
    pub position_x: i64,
    pub position_y: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub workspaces: Vec<NiriWorkspace>,
    pub windows: Vec<NiriWindow>,
    pub active_workspace_id: Option<u64>,
    pub active_workspace_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NiriUpdate {
    Snapshot(WorkspaceSnapshot),
    WindowChanged(NiriWindow),
    WindowClosed(u64),
    WindowFocusChanged(Option<u64>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawWorkspace {
    id: u64,
    idx: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    active_window_id: Option<u64>,
    #[serde(default)]
    urgent: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawWindow {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    workspace_id: Option<u64>,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_floating: bool,
    #[serde(default)]
    layout: Option<RawWindowLayout>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RawWindowLayout {
    #[serde(default)]
    pos_in_scrolling_layout: Option<[i64; 2]>,
}

pub struct NiriClient;

impl NiriClient {
    pub fn snapshot() -> Result<WorkspaceSnapshot> {
        let workspaces = niri_json("workspaces").context("failed to query niri workspaces")?;
        let windows = niri_json("windows").context("failed to query niri windows")?;
        snapshot_from_json(&workspaces, &windows)
    }

    pub fn focus_workspace(idx: i64) -> Result<()> {
        niri_action(["focus-workspace", &idx.to_string()])
    }

    pub fn focus_window(id: u64) -> Result<()> {
        niri_action(["focus-window", "--id", &id.to_string()])
    }

    pub fn recenter_columns() -> Result<()> {
        niri_action(["expand-column-to-available-width"])
    }

    pub async fn stream_updates(sender: mpsc::UnboundedSender<NiriUpdate>) -> Result<()> {
        let mut child = TokioCommand::new("niri")
            .args(["msg", "--json", "event-stream"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to run niri msg --json event-stream")?;

        let stdout = child
            .stdout
            .take()
            .context("niri event-stream did not provide stdout")?;
        let mut lines = BufReader::new(stdout).lines();

        let mut snapshot = Self::snapshot().unwrap_or_default();
        let mut last_title_send_by_window = HashMap::new();

        while let Some(line) = lines
            .next_line()
            .await
            .context("failed to read niri event-stream")?
        {
            if line.trim().is_empty() {
                continue;
            }

            let Some(update) = apply_event(&mut snapshot, &line, &mut last_title_send_by_window)
                .context("failed to apply niri event")?
            else {
                continue;
            };

            if sender.send(update).is_err() {
                let _ = child.kill().await;
                return Ok(());
            }
        }

        let status = child
            .wait()
            .await
            .context("failed to wait for niri event-stream")?;
        if !status.success() {
            anyhow::bail!("niri event-stream exited with status {status}");
        }

        Ok(())
    }
}

fn apply_event(
    snapshot: &mut WorkspaceSnapshot,
    line: &str,
    last_title_send_by_window: &mut HashMap<u64, Instant>,
) -> Result<Option<NiriUpdate>> {
    let event: Value = serde_json::from_str(line).context("failed to parse niri event")?;

    if let Some(payload) = event.get("WorkspacesChanged") {
        let workspaces = payload
            .get("workspaces")
            .cloned()
            .context("WorkspacesChanged missing workspaces")?;
        snapshot.workspaces = sort_workspaces(
            serde_json::from_value::<Vec<RawWorkspace>>(workspaces)?
                .into_iter()
                .map(map_workspace)
                .collect(),
        );
        refresh_active_workspace(snapshot);
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    if let Some(payload) = event.get("WorkspaceActivated") {
        let id = payload
            .get("id")
            .and_then(Value::as_u64)
            .context("WorkspaceActivated missing id")?;
        let focused = payload
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        activate_workspace(snapshot, id, focused);
        refresh_active_workspace(snapshot);
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    if let Some(payload) = event.get("WorkspaceActiveWindowChanged") {
        let workspace_id = payload
            .get("workspace_id")
            .and_then(Value::as_u64)
            .context("WorkspaceActiveWindowChanged missing workspace_id")?;
        let active_window_id = payload.get("active_window_id").and_then(Value::as_u64);
        if let Some(workspace) = snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
        {
            workspace.active_window_id = active_window_id;
        }
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    if let Some(payload) = event.get("WorkspaceUrgencyChanged") {
        let id = payload
            .get("id")
            .and_then(Value::as_u64)
            .context("WorkspaceUrgencyChanged missing id")?;
        let urgent = payload
            .get("urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(workspace) = snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
        {
            workspace.urgent = urgent;
        }
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    if let Some(payload) = event.get("WindowsChanged") {
        let windows = payload
            .get("windows")
            .cloned()
            .context("WindowsChanged missing windows")?;
        snapshot.windows = sort_windows(
            serde_json::from_value::<Vec<RawWindow>>(windows)?
                .into_iter()
                .map(map_window)
                .collect(),
        );
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    if let Some(payload) = event.get("WindowOpenedOrChanged") {
        let raw = payload
            .get("window")
            .cloned()
            .context("WindowOpenedOrChanged missing window")?;
        let window = map_window(serde_json::from_value(raw)?);
        if snapshot
            .windows
            .iter()
            .any(|existing| existing == &window)
        {
            return Ok(None);
        }
        let title_only = snapshot
            .windows
            .iter()
            .find(|existing| existing.id == window.id)
            .is_some_and(|existing| title_only_changed(existing, &window));
        upsert_window(snapshot, window.clone());
        if title_only {
            let last_send = last_title_send_by_window
                .entry(window.id)
                .or_insert_with(|| Instant::now() - Duration::from_secs(1));
            if last_send.elapsed() < Duration::from_secs(1) {
                return Ok(None);
            }
            *last_send = Instant::now();
        } else {
            last_title_send_by_window.remove(&window.id);
        }
        return Ok(Some(NiriUpdate::WindowChanged(window)));
    }

    if let Some(payload) = event.get("WindowClosed") {
        let id = payload
            .get("id")
            .and_then(Value::as_u64)
            .context("WindowClosed missing id")?;
        snapshot.windows.retain(|window| window.id != id);
        last_title_send_by_window.remove(&id);
        for workspace in &mut snapshot.workspaces {
            if workspace.active_window_id == Some(id) {
                workspace.active_window_id = None;
            }
        }
        return Ok(Some(NiriUpdate::WindowClosed(id)));
    }

    if let Some(payload) = event.get("WindowFocusChanged") {
        let id = payload.get("id").and_then(Value::as_u64);
        for window in &mut snapshot.windows {
            window.focused = Some(window.id) == id;
        }
        return Ok(Some(NiriUpdate::WindowFocusChanged(id)));
    }

    if let Some(payload) = event.get("WindowUrgencyChanged") {
        let id = payload
            .get("id")
            .and_then(Value::as_u64)
            .context("WindowUrgencyChanged missing id")?;
        let urgent = payload
            .get("urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if urgent {
            if let Some(workspace_id) = snapshot
                .windows
                .iter()
                .find(|window| window.id == id)
                .and_then(|window| window.workspace_id)
                && let Some(workspace) = snapshot
                    .workspaces
                    .iter_mut()
                    .find(|workspace| workspace.id == workspace_id)
            {
                workspace.urgent = true;
            }
        }
        return Ok(Some(NiriUpdate::Snapshot(snapshot.clone())));
    }

    Ok(None)
}

fn title_only_changed(previous: &NiriWindow, next: &NiriWindow) -> bool {
    previous.title != next.title
        && previous.id == next.id
        && previous.app_id == next.app_id
        && previous.workspace_id == next.workspace_id
        && previous.focused == next.focused
        && previous.floating == next.floating
        && previous.position_x == next.position_x
        && previous.position_y == next.position_y
}

fn niri_json(kind: &str) -> Result<String> {
    let output = Command::new("niri")
        .args(["msg", "--json", kind])
        .output()
        .with_context(|| format!("failed to run niri msg --json {kind}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "niri msg --json {kind} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn niri_action<const N: usize>(args: [&str; N]) -> Result<()> {
    let output = Command::new("niri")
        .args(["msg", "action"])
        .args(args)
        .output()
        .context("failed to run niri action")?;
    if !output.status.success() {
        anyhow::bail!(
            "niri action failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn snapshot_from_json(workspaces_json: &str, windows_json: &str) -> Result<WorkspaceSnapshot> {
    let workspaces: Vec<NiriWorkspace> =
        serde_json::from_str::<Vec<RawWorkspace>>(workspaces_json)?
            .into_iter()
            .map(map_workspace)
            .collect();

    let windows: Vec<NiriWindow> = serde_json::from_str::<Vec<RawWindow>>(windows_json)?
        .into_iter()
        .map(map_window)
        .collect();

    Ok(snapshot_from_parts(
        sort_workspaces(workspaces),
        sort_windows(windows),
    ))
}

fn snapshot_from_parts(
    workspaces: Vec<NiriWorkspace>,
    windows: Vec<NiriWindow>,
) -> WorkspaceSnapshot {
    let mut snapshot = WorkspaceSnapshot {
        workspaces,
        windows,
        ..WorkspaceSnapshot::default()
    };
    refresh_active_workspace(&mut snapshot);
    snapshot
}

fn sort_workspaces(mut workspaces: Vec<NiriWorkspace>) -> Vec<NiriWorkspace> {
    workspaces.sort_by(|a, b| {
        a.output
            .cmp(&b.output)
            .then(a.idx.cmp(&b.idx))
            .then(a.id.cmp(&b.id))
    });
    workspaces
}

fn sort_windows(mut windows: Vec<NiriWindow>) -> Vec<NiriWindow> {
    windows.sort_by(|a, b| {
        a.workspace_id
            .cmp(&b.workspace_id)
            .then(a.position_x.cmp(&b.position_x))
            .then(a.position_y.cmp(&b.position_y))
            .then(a.id.cmp(&b.id))
    });
    windows
}

fn refresh_active_workspace(snapshot: &mut WorkspaceSnapshot) {
    let active = snapshot
        .workspaces
        .iter()
        .find(|w| matches!(w.focus, WorkspaceFocus::Focused));
    let active = active.or_else(|| {
        snapshot
            .workspaces
            .iter()
            .find(|w| matches!(w.focus, WorkspaceFocus::Active))
    });

    snapshot.active_workspace_id = active.map(|w| w.id);
    snapshot.active_workspace_label = active
        .map(|w| w.label.clone())
        .unwrap_or_else(|| "Workspace".to_string());
}

fn activate_workspace(snapshot: &mut WorkspaceSnapshot, id: u64, focused: bool) {
    let output = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .map(|workspace| workspace.output.clone());

    for workspace in &mut snapshot.workspaces {
        if focused && matches!(workspace.focus, WorkspaceFocus::Focused) {
            workspace.focus = WorkspaceFocus::Inactive;
        }

        if output
            .as_ref()
            .is_some_and(|output| *output == workspace.output && workspace.focus.is_current())
        {
            workspace.focus = WorkspaceFocus::Inactive;
        }

        if workspace.id == id {
            workspace.focus = if focused {
                WorkspaceFocus::Focused
            } else {
                WorkspaceFocus::Active
            };
        }
    }
}

fn upsert_window(snapshot: &mut WorkspaceSnapshot, window: NiriWindow) {
    if window.focused {
        for existing in &mut snapshot.windows {
            existing.focused = false;
        }
    }

    if let Some(existing) = snapshot
        .windows
        .iter_mut()
        .find(|existing| existing.id == window.id)
    {
        *existing = window;
    } else {
        snapshot.windows.push(window);
    }
    snapshot.windows = sort_windows(std::mem::take(&mut snapshot.windows));
}

fn map_workspace(raw: RawWorkspace) -> NiriWorkspace {
    let name = raw.name.unwrap_or_default();
    let label = if name.is_empty() {
        raw.idx.to_string()
    } else {
        name.clone()
    };
    let focus = if raw.is_focused {
        WorkspaceFocus::Focused
    } else if raw.is_active {
        WorkspaceFocus::Active
    } else {
        WorkspaceFocus::Inactive
    };
    NiriWorkspace {
        id: raw.id,
        key: workspace_key(raw.id),
        idx: raw.idx,
        name,
        label,
        output: raw.output.unwrap_or_default(),
        focus,
        urgent: raw.urgent,
        active_window_id: raw.active_window_id,
    }
}

fn map_window(raw: RawWindow) -> NiriWindow {
    let position = raw
        .layout
        .and_then(|layout| layout.pos_in_scrolling_layout)
        .unwrap_or([0, 0]);
    NiriWindow {
        id: raw.id,
        key: window_key(raw.id),
        title: raw.title.unwrap_or_else(|| "(untitled)".to_string()),
        app_id: raw.app_id.unwrap_or_default(),
        workspace_id: raw.workspace_id,
        focused: raw.is_focused,
        floating: raw.is_floating,
        position_x: position[0],
        position_y: position[1],
    }
}

pub fn workspace_key(id: u64) -> String {
    format!("niri:workspace:{id}")
}

pub fn window_key(id: u64) -> String {
    format!("niri:window:{id}")
}

pub fn app_initial(app_id: &str) -> String {
    let normalized = app_id
        .strip_prefix("com.")
        .or_else(|| app_id.strip_prefix("org."))
        .unwrap_or(app_id);
    normalized
        .split(['.', '-', '_', ' '])
        .filter(|part| !part.is_empty())
        .next_back()
        .and_then(|token| token.chars().next())
        .map(|ch| ch.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string())
}

pub fn app_icon_path(app_id: &str) -> Option<PathBuf> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();

    if app_id.trim().is_empty() {
        return None;
    }

    let key = app_id.to_string();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?;
    if let Some(path) = cache.get(&key) {
        return path.clone();
    }

    let path = resolve_app_icon_path(app_id);
    cache.insert(key, path.clone());
    path
}

pub fn agent_workspace_key(active_workspace_id: Option<u64>) -> String {
    active_workspace_id
        .map(workspace_key)
        .unwrap_or_else(|| "workspace:default".to_string())
}

fn resolve_app_icon_path(app_id: &str) -> Option<PathBuf> {
    let mut icon_names = Vec::new();
    if let Some(icon) = desktop_entry_icon(app_id) {
        icon_names.push(icon);
    }
    icon_names.push(app_id.to_string());

    let normalized = app_id
        .strip_prefix("com.")
        .or_else(|| app_id.strip_prefix("org."))
        .unwrap_or(app_id);
    icon_names.push(normalized.to_string());
    icon_names.extend(
        normalized
            .split(['.', '-', '_', ' '])
            .filter(|part| !part.is_empty())
            .rev()
            .map(|part| part.to_lowercase()),
    );

    let mut seen = HashSet::new();
    icon_names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .find_map(|name| find_icon_path(&name))
}

fn desktop_entry_icon(app_id: &str) -> Option<String> {
    let app_id_lower = app_id.to_lowercase();
    for dir in data_dirs() {
        let applications = dir.join("applications");
        let Ok(entries) = fs::read_dir(applications) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let stem_lower = stem.to_lowercase();
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let startup_wm_class = desktop_value(&contents, "StartupWMClass")
                .map(|value| value.to_lowercase())
                .unwrap_or_default();
            if stem_lower == app_id_lower
                || stem_lower == format!("{app_id_lower}.desktop")
                || startup_wm_class == app_id_lower
            {
                return desktop_value(&contents, "Icon").filter(|icon| !icon.is_empty());
            }
        }
    }
    None
}

fn desktop_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

fn find_icon_path(icon: &str) -> Option<PathBuf> {
    let path = Path::new(icon);
    if path.is_absolute() && path.exists() {
        return Some(path.to_path_buf());
    }

    let mut names = vec![icon.to_string()];
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        names.push(stem.to_string());
    }

    let extensions = ["svg", "png", "xpm"];
    let mut seen = HashSet::new();
    let names: Vec<String> = names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect();
    for dir in icon_search_dirs() {
        for name in &names {
            for extension in extensions {
                let direct = dir.join(format!("{name}.{extension}"));
                if direct.exists() {
                    return Some(direct);
                }
            }
        }
        if let Some(found) = find_icon_path_recursive(&dir, &names, &extensions, 5) {
            return Some(found);
        }
    }
    None
}

fn find_icon_path_recursive(
    dir: &Path,
    names: &[String],
    extensions: &[&str],
    depth: usize,
) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_icon_path_recursive(&path, names, extensions, depth - 1) {
                return Some(found);
            }
            continue;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let extension = path.extension().and_then(|extension| extension.to_str());
        if stem.is_some_and(|stem| names.iter().any(|name| name == stem))
            && extension.is_some_and(|extension| extensions.contains(&extension))
        {
            return Some(path);
        }
    }
    None
}

fn icon_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for data_dir in data_dirs() {
        dirs.push(data_dir.join("pixmaps"));
        dirs.push(data_dir.join("icons"));
    }
    dirs
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    if let Some(nix_profile) = env::var_os("HOME") {
        dirs.push(PathBuf::from(nix_profile).join(".nix-profile/share"));
    }
    if let Some(user) = env::var_os("USER") {
        dirs.push(
            PathBuf::from("/etc/profiles/per-user")
                .join(user)
                .join("share"),
        );
    }
    dirs.push(PathBuf::from("/run/current-system/sw/share"));
    let xdg_data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| {
            env::split_paths(&value)
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(xdg_data_dirs);

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.clone()))
        .collect()
}

pub trait ShellWindow {
    fn workspace_id(&self) -> Option<u64>;
}

impl ShellWindow for NiriWindow {
    fn workspace_id(&self) -> Option<u64> {
        self.workspace_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_qml_sorting_and_labels() {
        let workspaces = r#"[
          {"id":2,"idx":2,"output":"HDMI-A-1","is_active":false},
          {"id":1,"idx":1,"name":"dev","output":"eDP-1","is_focused":true}
        ]"#;
        let windows = r#"[
          {"id":12,"title":"B","app_id":"org.example.B","workspace_id":1,"layout":{"pos_in_scrolling_layout":[20,0]}},
          {"id":11,"title":"A","app_id":"com.example.A","workspace_id":1,"is_focused":true,"layout":{"pos_in_scrolling_layout":[10,0]}}
        ]"#;

        let snapshot = snapshot_from_json(workspaces, windows).unwrap();

        assert_eq!(snapshot.active_workspace_id, Some(1));
        assert_eq!(snapshot.active_workspace_label, "dev");
        assert_eq!(
            snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.id == 1)
                .unwrap()
                .label,
            "dev"
        );
        assert_eq!(
            snapshot.windows.iter().map(|w| w.id).collect::<Vec<_>>(),
            vec![11, 12]
        );
    }

    #[test]
    fn snapshot_keeps_output_separate_from_workspace_label() {
        let workspaces = r#"[
          {"id":1,"idx":1,"name":"web","output":"eDP-1","is_focused":true},
          {"id":2,"idx":2,"output":"HDMI-A-1","is_active":false}
        ]"#;
        let windows = "[]";

        let snapshot = snapshot_from_json(workspaces, windows).unwrap();
        let named = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == 1)
            .unwrap();
        let unnamed = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == 2)
            .unwrap();

        assert_eq!(named.label, "web");
        assert_eq!(named.output, "eDP-1");
        assert_eq!(unnamed.label, "2");
        assert_eq!(unnamed.output, "HDMI-A-1");
    }

    #[test]
    fn app_initial_matches_sidebar_fallback() {
        assert_eq!(app_initial("com.github.wez.wezterm"), "W");
        assert_eq!(app_initial(""), "?");
    }
}
