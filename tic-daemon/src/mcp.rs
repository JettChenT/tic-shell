use std::{
    collections::HashMap,
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use base64::Engine;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::io::stdio,
};
use serde::{Deserialize, Serialize};

use crate::niri::{LogicalOutput, Niri, ScrollDirection, Window, Workspace};

#[derive(Debug, Deserialize, JsonSchema)]
struct EmitEventParams {
    #[schemars(
        description = "Machine-readable event type, for example window_heartbeat_l1_update."
    )]
    event_type: String,
    #[schemars(description = "Textual description of what happened.")]
    description: String,
    #[schemars(description = "Optional niri window id.")]
    window_id: Option<u64>,
    #[schemars(description = "Optional niri workspace id.")]
    workspace_id: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetWindowDescriptionParams {
    #[schemars(description = "Niri window id to describe.")]
    window_id: u64,
    #[schemars(
        description = "Very short present-tense description of what is happening in the window. Only set this when the existing window title is not descriptive enough; keep it easy to scan at a glance. Send an empty string to clear it."
    )]
    description: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SetWorkspaceNameParams {
    #[schemars(description = "Niri workspace id to name.")]
    workspace_id: u64,
    #[schemars(
        description = "Short human-readable workspace name. Use this when a workspace is currently unnamed and its activity has become clear."
    )]
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DescribeWorkspaceParams {
    workspace_id: Option<u64>,
    include_screenshots: Option<bool>,
    intrusive_fallback: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ViewWindowParams {
    window_id: u64,
    intrusive_fallback: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ClickParams {
    window_id: u64,
    x: u32,
    y: u32,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TypeTextParams {
    window_id: u64,
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PressKeyParams {
    window_id: u64,
    key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ScrollParams {
    window_id: u64,
    direction: McpScrollDirection,
    amount: u32,
    x: Option<u32>,
    y: Option<u32>,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(rename_all = "lowercase")]
enum McpScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CloseSessionParams {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct DescribeWorkspaceOutput {
    compositor: &'static str,
    workspace: Workspace,
    screenshot_dir: Option<PathBuf>,
    composite_screenshot: Option<PathBuf>,
    windows: Vec<WindowInfo>,
}

#[derive(Debug, Serialize)]
struct WindowInfo {
    id: u64,
    title: String,
    app_id: String,
    pid: Option<i32>,
    is_focused: bool,
    is_floating: bool,
    screenshot: Option<PathBuf>,
    screenshot_error: Option<String>,
    size: [i32; 2],
    screenshot_size: Option<[u32; 2]>,
    scale: Option<f64>,
    coordinate_space: &'static str,
}

#[derive(Debug, Serialize)]
struct ScreenshotOutput {
    window_id: u64,
    path: PathBuf,
    size: Option<[u32; 2]>,
    scale: Option<f64>,
    coordinate_space: &'static str,
}

struct CuaSession {
    session_id: String,
    cursor_id: String,
    generated: bool,
}

impl CuaSession {
    fn new(session_id: Option<String>) -> Result<Self> {
        let (session_id, generated) = match session_id {
            Some(session_id) => (normalize_session_id(session_id)?, false),
            None => match env::var("CUA_SESSION_ID") {
                Ok(session_id) if !session_id.trim().is_empty() => {
                    (normalize_session_id(session_id)?, false)
                }
                _ => (generate_session_id()?, true),
            },
        };
        let cursor_id = format!("tic-cua-mcp-{session_id}");
        Ok(Self {
            session_id,
            cursor_id,
            generated,
        })
    }

    fn reuse_hint(&self) -> String {
        if self.generated {
            format!(
                "Pass session_id {:?} on future click/scroll calls for this task, then call close-session with it when finished.",
                self.session_id
            )
        } else {
            format!(
                "Continue passing session_id {:?} for this task and call close-session with it when finished.",
                self.session_id
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct TicMcpServer {
    event_log: PathBuf,
}

impl TicMcpServer {
    pub fn new(event_log: PathBuf) -> Self {
        Self { event_log }
    }

    fn append_event(&self, params: EmitEventParams) -> Result<CallToolResult> {
        if let Some(parent) = self.event_log.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let line = serde_json::json!({
            "time": chrono::Utc::now().to_rfc3339(),
            "type": params.event_type,
            "description": params.description,
            "window_id": params.window_id,
            "workspace_id": params.workspace_id,
        });
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.event_log)
            .with_context(|| format!("open {}", self.event_log.display()))?;
        writeln!(file, "{line}")?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({ "emitted": true, "event": line }).to_string(),
        )]))
    }

    fn append_json_event(&self, line: serde_json::Value) -> Result<()> {
        if let Some(parent) = self.event_log.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.event_log)
            .with_context(|| format!("open {}", self.event_log.display()))?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    fn set_window_description(&self, params: SetWindowDescriptionParams) -> Result<CallToolResult> {
        let description = normalize_short_description(&params.description);
        self.append_json_event(serde_json::json!({
            "time": chrono::Utc::now().to_rfc3339(),
            "type": "window_description_set",
            "window_id": params.window_id,
            "description": description,
        }))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({
                "window_id": params.window_id,
                "description": description,
                "storage": "memory",
            })
            .to_string(),
        )]))
    }

    fn set_workspace_name(&self, params: SetWorkspaceNameParams) -> Result<CallToolResult> {
        let name = normalize_workspace_name(&params.name);
        if name.is_empty() {
            anyhow::bail!("workspace name cannot be empty");
        }
        self.append_json_event(serde_json::json!({
            "time": chrono::Utc::now().to_rfc3339(),
            "type": "workspace_name_set",
            "workspace_id": params.workspace_id,
            "name": name,
            "description": format!("workspace {} named {}", params.workspace_id, name),
        }))?;

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({
                "workspace_id": params.workspace_id,
                "name": name,
                "storage": "sidebar_annotation",
            })
            .to_string(),
        )]))
    }

    fn describe_workspace_inner(&self, params: DescribeWorkspaceParams) -> Result<CallToolResult> {
        let niri = Niri::discover()?;
        let _intrusive_fallback = params.intrusive_fallback;
        let output = describe_workspace(
            &niri,
            params.workspace_id,
            params.include_screenshots.unwrap_or(false),
        )?;
        let mut content = vec![Content::text(serde_json::to_string_pretty(&output)?)];
        if let Some(path) = &output.composite_screenshot {
            content.push(image_content(path)?);
        }
        Ok(CallToolResult::success(content))
    }

    fn view_window_inner(&self, params: ViewWindowParams) -> Result<CallToolResult> {
        let niri = Niri::discover()?;
        let path = screenshot_window(
            &niri,
            params.window_id,
            &capture_dir()?,
            params.intrusive_fallback.unwrap_or(false),
        )?;
        let window = niri.window(params.window_id)?;
        let output = ScreenshotOutput {
            window_id: params.window_id,
            path: path.clone(),
            size: image_size(&path).ok(),
            scale: niri.window_scale(&window).ok(),
            coordinate_space: "screenshot_pixels",
        };
        Ok(CallToolResult::success(vec![
            Content::text(serde_json::to_string_pretty(&output)?),
            image_content(&path)?,
        ]))
    }
}

#[tool_router]
impl TicMcpServer {
    #[tool(
        name = "emit_event",
        description = "Emit a daemon Heart event with a textual description."
    )]
    fn emit_event(&self, Parameters(params): Parameters<EmitEventParams>) -> CallToolResult {
        self.append_event(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "set-window-description",
        description = "Set or clear a very short live description for a niri window. Only use this when the existing window title is not descriptive enough, and keep the text easy to scan at a glance."
    )]
    fn set_window_description_tool(
        &self,
        Parameters(params): Parameters<SetWindowDescriptionParams>,
    ) -> CallToolResult {
        self.set_window_description(params)
            .unwrap_or_else(tool_error)
    }

    #[tool(
        name = "set-workspace-name",
        description = "Set a short human-readable name for a niri workspace. Prefer using this from L2 workspace heartbeat sessions when the workspace is unnamed and the activity is clear."
    )]
    fn set_workspace_name_tool(
        &self,
        Parameters(params): Parameters<SetWorkspaceNameParams>,
    ) -> CallToolResult {
        self.set_workspace_name(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "describe-workspace",
        description = "Return niri workspace/window metadata through the daemon CUA compatibility layer."
    )]
    fn describe_workspace(
        &self,
        Parameters(params): Parameters<DescribeWorkspaceParams>,
    ) -> CallToolResult {
        self.describe_workspace_inner(params)
            .unwrap_or_else(tool_error)
    }

    #[tool(
        name = "view-window",
        description = "Capture a single niri window and return its screenshot path through the daemon CUA compatibility layer."
    )]
    fn view_window(&self, Parameters(params): Parameters<ViewWindowParams>) -> CallToolResult {
        self.view_window_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "click",
        description = "Click inside a window at screenshot pixel coordinates."
    )]
    fn click(&self, Parameters(params): Parameters<ClickParams>) -> CallToolResult {
        click_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(name = "type-text", description = "Type text into a window.")]
    fn type_text(&self, Parameters(params): Parameters<TypeTextParams>) -> CallToolResult {
        type_text_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(name = "press-key", description = "Press one named key in a window.")]
    fn press_key(&self, Parameters(params): Parameters<PressKeyParams>) -> CallToolResult {
        press_key_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(name = "scroll", description = "Scroll inside a window.")]
    fn scroll(&self, Parameters(params): Parameters<ScrollParams>) -> CallToolResult {
        scroll_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(name = "close-session", description = "Close a CUA mouse session.")]
    fn close_session(&self, Parameters(params): Parameters<CloseSessionParams>) -> CallToolResult {
        let cursor_id = format!("tic-cua-mcp-{}", params.session_id);
        match crate::niri::Niri::discover().and_then(|niri| niri.destroy_virtual_cursor(&cursor_id)) {
            Ok(()) => CallToolResult::success(vec![Content::text(
                serde_json::json!({ "session_id": params.session_id, "virtual_cursor": cursor_id, "closed": true }).to_string(),
            )]),
            Err(err) => tool_error(err),
        }
    }
}

impl From<McpScrollDirection> for ScrollDirection {
    fn from(direction: McpScrollDirection) -> Self {
        match direction {
            McpScrollDirection::Up => ScrollDirection::Up,
            McpScrollDirection::Down => ScrollDirection::Down,
            McpScrollDirection::Left => ScrollDirection::Left,
            McpScrollDirection::Right => ScrollDirection::Right,
        }
    }
}

#[tool_handler(name = "tic-daemon", version = "0.1.0")]
impl ServerHandler for TicMcpServer {}

pub async fn run_mcp(event_log: PathBuf) -> Result<()> {
    TicMcpServer::new(event_log)
        .serve(stdio())
        .await?
        .waiting()
        .await
        .context("run tic-daemon MCP server")?;
    Ok(())
}

fn tool_error(err: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        err.chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(": "),
    )])
}

fn image_content(path: &Path) -> Result<Content> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(Content::image(
        base64::engine::general_purpose::STANDARD.encode(data),
        "image/png",
    ))
}

fn describe_workspace(
    niri: &Niri,
    workspace_id: Option<u64>,
    include_screenshots: bool,
) -> Result<DescribeWorkspaceOutput> {
    let originally_focused_window = niri.focused_window().ok().map(|window| window.id);
    let workspaces = niri.workspaces()?;
    let id = workspace_id
        .or_else(env_workspace_id)
        .or_else(|| workspaces.iter().find(|w| w.is_focused).map(|w| w.id))
        .or_else(|| workspaces.iter().find(|w| w.is_active).map(|w| w.id))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no workspace id was provided and niri did not report an active workspace"
            )
        })?;

    let workspace = workspaces
        .iter()
        .find(|w| w.id == id || w.idx == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("workspace {id} was not found by id or idx"))?;
    let output_scales: HashMap<String, f64> = niri
        .outputs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|output| output.logical.map(|logical| (output.name, logical.scale)))
        .collect();
    let workspace_scales: HashMap<u64, f64> = workspaces
        .iter()
        .filter_map(|workspace| {
            workspace
                .output
                .as_ref()
                .and_then(|output| output_scales.get(output).copied())
                .map(|scale| (workspace.id, scale))
        })
        .collect();

    let dir = if include_screenshots {
        Some(capture_dir()?)
    } else {
        None
    };
    let composite_screenshot = if let Some(dir) = &dir {
        let path = dir.join(format!("workspace-{}-composite.png", workspace.id));
        niri.screenshot_workspace(workspace.id, &path)?;
        Some(path)
    } else {
        None
    };
    let windows: Vec<Window> = niri
        .windows()?
        .into_iter()
        .filter(|window| window.workspace_id == Some(workspace.id))
        .collect();

    let mut infos = Vec::with_capacity(windows.len());
    for window in windows {
        let scale = window
            .workspace_id
            .and_then(|workspace_id| workspace_scales.get(&workspace_id).copied());
        infos.push(WindowInfo {
            id: window.id,
            title: window.title.unwrap_or_else(|| "(untitled)".to_string()),
            app_id: window.app_id.unwrap_or_default(),
            pid: window.pid,
            is_focused: window.is_focused,
            is_floating: window.is_floating,
            screenshot: None,
            screenshot_error: None,
            size: window.layout.window_size,
            screenshot_size: None,
            scale,
            coordinate_space: "screenshot_pixels",
        });
    }

    if let Some(window_id) = originally_focused_window {
        let _ = niri.focus_window(window_id);
    }

    Ok(DescribeWorkspaceOutput {
        compositor: "niri",
        workspace,
        screenshot_dir: dir,
        composite_screenshot,
        windows: infos,
    })
}

fn screenshot_window(
    niri: &Niri,
    window_id: u64,
    dir: &Path,
    intrusive_fallback: bool,
) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(format!("window-{window_id}.png"));
    match niri.screenshot_window(window_id, &path) {
        Ok(()) => return Ok(path),
        Err(err) if !intrusive_fallback => {
            return Err(err).with_context(|| {
                "CUA-aligned niri screenshot failed; pass intrusive_fallback=true to use grim after focusing the window"
            });
        }
        Err(_) => {
            let _ = fs::remove_file(&path);
        }
    }

    screenshot_window_with_grim(niri, window_id, &path)?;
    Ok(path)
}

fn screenshot_window_with_grim(niri: &Niri, window_id: u64, path: &Path) -> Result<()> {
    let (x, y, width, height) = focus_and_resolve_rect(niri, window_id)?;
    niri.grim(
        &format!("{x},{y} {width}x{height}"),
        path.to_str()
            .ok_or_else(|| anyhow::anyhow!("non-utf8 screenshot path"))?,
    )
}

fn focus_and_resolve_rect(niri: &Niri, window_id: u64) -> Result<(i32, i32, u32, u32)> {
    niri.focus_window(window_id)?;
    thread::sleep(Duration::from_millis(160));

    let window = niri.window(window_id)?;
    let output = niri.logical_output_for_window(&window)?;
    Ok(resolve_window_rect(&output, &window))
}

fn resolve_window_rect(output: &LogicalOutput, window: &Window) -> (i32, i32, u32, u32) {
    let width = window_width(window).unwrap_or(1);
    let height = window_height(window).unwrap_or(1);
    let (x, y) = if let Some(tile_pos) = window.layout.tile_pos_in_workspace_view {
        (
            output.x + (tile_pos[0] + window.layout.window_offset_in_tile[0]).round() as i32,
            output.y + (tile_pos[1] + window.layout.window_offset_in_tile[1]).round() as i32,
        )
    } else {
        (
            output.x + ((output.width.saturating_sub(width)) / 2) as i32,
            output.y + ((output.height.saturating_sub(height)) / 2) as i32,
        )
    };
    (x, y, width, height)
}

fn window_width(window: &Window) -> Result<u32> {
    u32::try_from(window.layout.window_size[0])
        .ok()
        .filter(|width| *width > 0)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "window {} has invalid width {}",
                window.id,
                window.layout.window_size[0]
            )
        })
}

fn window_height(window: &Window) -> Result<u32> {
    u32::try_from(window.layout.window_size[1])
        .ok()
        .filter(|height| *height > 0)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "window {} has invalid height {}",
                window.id,
                window.layout.window_size[1]
            )
        })
}

fn image_size(path: &Path) -> Result<[u32; 2]> {
    let (width, height) =
        image::image_dimensions(path).with_context(|| format!("read {}", path.display()))?;
    Ok([width, height])
}

fn capture_dir() -> Result<PathBuf> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let dir = env::temp_dir().join(format!("tic-cua-{}-{now}", std::process::id()));
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o777))
        .with_context(|| format!("set permissions on {}", dir.display()))?;
    Ok(dir)
}

fn env_workspace_id() -> Option<u64> {
    env::var("CUA_WORKSPACE_ID")
        .ok()
        .or_else(|| env::var("NIRI_WORKSPACE_ID").ok())
        .and_then(|value| value.parse().ok())
}

fn click_inner(params: ClickParams) -> Result<CallToolResult> {
    let niri = Niri::discover()?;
    let (logical_x, logical_y, scale) =
        niri.screenshot_to_logical(params.window_id, params.x, params.y)?;
    let session = CuaSession::new(params.session_id)?;
    niri.virtual_cursor_click(&session.cursor_id, params.window_id, logical_x, logical_y)?;
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::json!({
            "window_id": params.window_id,
            "clicked": { "x": params.x, "y": params.y },
            "coordinate_space": "screenshot_pixels",
            "sent_logical": { "x": logical_x, "y": logical_y },
            "session_id": session.session_id,
            "session_id_generated": session.generated,
            "session_reuse_hint": session.reuse_hint(),
            "virtual_cursor": session.cursor_id,
            "scale": scale
        })
        .to_string(),
    )]))
}

fn type_text_inner(params: TypeTextParams) -> Result<CallToolResult> {
    let niri = Niri::discover()?;
    niri.cua_type_text(params.window_id, &params.text)?;
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::json!({
            "window_id": params.window_id,
            "typed_chars": params.text.chars().count()
        })
        .to_string(),
    )]))
}

fn press_key_inner(params: PressKeyParams) -> Result<CallToolResult> {
    let niri = Niri::discover()?;
    niri.cua_press_key(params.window_id, &params.key)?;
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::json!({
            "window_id": params.window_id,
            "pressed_key": params.key
        })
        .to_string(),
    )]))
}

fn scroll_inner(params: ScrollParams) -> Result<CallToolResult> {
    let niri = Niri::discover()?;
    let direction = params.direction.into();
    let transformed = niri.scroll_target(params.window_id, params.x, params.y)?;
    let session = CuaSession::new(params.session_id)?;
    niri.virtual_cursor_scroll(
        &session.cursor_id,
        params.window_id,
        direction,
        params.amount,
        transformed.logical_x,
        transformed.logical_y,
    )?;
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::json!({
            "window_id": params.window_id,
            "scrolled": params.amount,
            "coordinate_space": "screenshot_pixels",
            "sent_logical": { "x": transformed.logical_x, "y": transformed.logical_y },
            "session_id": session.session_id,
            "session_id_generated": session.generated,
            "session_reuse_hint": session.reuse_hint(),
            "virtual_cursor": session.cursor_id,
            "scale": transformed.scale
        })
        .to_string(),
    )]))
}

fn generate_session_id() -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_millis();
    Ok(format!("s{}-{now}", std::process::id()))
}

fn normalize_session_id(session_id: String) -> Result<String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        anyhow::bail!("session_id must not be empty");
    }
    if !session_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        anyhow::bail!("session_id may only contain ASCII letters, digits, '-' and '_'");
    }
    Ok(session_id.to_owned())
}

fn normalize_short_description(input: &str) -> String {
    let mut normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_CHARS: usize = 80;
    if normalized.chars().count() > MAX_CHARS {
        normalized = normalized.chars().take(MAX_CHARS - 1).collect::<String>();
        normalized.push('…');
    }
    normalized
}

fn normalize_workspace_name(input: &str) -> String {
    let mut normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_CHARS: usize = 40;
    if normalized.chars().count() > MAX_CHARS {
        normalized = normalized.chars().take(MAX_CHARS - 1).collect::<String>();
        normalized.push('…');
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_router_exposes_emit_event() {
        let names = TicMcpServer::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "emit_event"));
        assert!(names.iter().any(|name| name == "set-window-description"));
        assert!(names.iter().any(|name| name == "set-workspace-name"));
        assert!(names.iter().any(|name| name == "view-window"));
        assert!(names.iter().any(|name| name == "describe-workspace"));
    }

    #[test]
    fn short_description_is_normalized_and_capped() {
        let input = format!("  compiling\n\n{}  ", "x".repeat(200));
        let output = normalize_short_description(&input);

        assert!(!output.contains('\n'));
        assert_eq!(output.chars().count(), 80);
        assert!(output.ends_with('…'));
    }

    #[test]
    fn workspace_name_is_normalized_and_capped() {
        let input = format!("  tic\n\n{}  ", "x".repeat(200));
        let output = normalize_workspace_name(&input);

        assert!(!output.contains('\n'));
        assert_eq!(output.chars().count(), 40);
        assert!(output.ends_with('…'));
    }
}
