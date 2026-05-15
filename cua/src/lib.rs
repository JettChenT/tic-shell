use anyhow::{Context, Result, anyhow};
use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::io::stdio,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const VIRTUAL_CURSOR_ID_PREFIX: &str = "tic-cua-mcp";
const VIRTUAL_CURSOR_THEME: &str = "Tiri-CUA";
const VIRTUAL_CURSOR_ICON: &str = "left_ptr";
const VIRTUAL_CURSOR_SIZE: u16 = 32;
const VIRTUAL_CURSOR_MOVE_MS: u32 = 120;
const CUA_SESSION_ID_ENV: &str = "CUA_SESSION_ID";
const CUA_CURSOR_THEME_ENV: &str = "CUA_CURSOR_THEME";
const CUA_CURSOR_SHAPE_ENV: &str = "CUA_CURSOR_SHAPE";
const CUA_CURSOR_COLOR_ENV: &str = "CUA_CURSOR_COLOR";
const CUA_CURSOR_OUTLINE_COLOR_ENV: &str = "CUA_CURSOR_OUTLINE_COLOR";

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Computer-use CLI for Wayland compositors, niri first"
)]
struct Cli {
    #[arg(long, global = true)]
    intrusive_fallback: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(alias = "server")]
    Mcp,
    #[command(alias = "describe_workspace")]
    DescribeWorkspace {
        workspace_id: Option<u64>,
        #[arg(long, help = "Capture one compositor-rendered workspace screenshot.")]
        screenshots: bool,
    },
    #[command(alias = "screenshot_window")]
    ScreenshotWindow {
        window_id: u64,
    },
    Click {
        window_id: u64,
        #[arg(help = "Window-relative X coordinate in screenshot/image pixels")]
        x: u32,
        #[arg(help = "Window-relative Y coordinate in screenshot/image pixels")]
        y: u32,
    },
    #[command(alias = "type")]
    TypeText {
        window_id: u64,
        text: String,
    },
    PressKey {
        window_id: u64,
        key: String,
    },
    Scroll {
        window_id: u64,
        direction: ScrollDirection,
        amount: u32,
        #[arg(help = "Optional window-relative X coordinate in screenshot/image pixels")]
        x: Option<u32>,
        #[arg(help = "Optional window-relative Y coordinate in screenshot/image pixels")]
        y: Option<u32>,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DescribeWorkspaceParams {
    /// Optional numeric niri workspace id or index, for example 1. Do not pass tic-shell keys like niri:workspace:1. Defaults to this MCP session's workspace via CUA_WORKSPACE_ID, then the focused workspace.
    workspace_id: Option<u64>,
    /// Capture each window and return a composite screenshot. Defaults to false because window metadata is much faster.
    include_screenshots: Option<bool>,
    /// Use grim after focusing windows if compositor-native screenshots are unavailable.
    intrusive_fallback: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ViewWindowParams {
    /// The numeric niri window id to capture.
    window_id: u64,
    /// Use grim after focusing the window if compositor-native screenshots are unavailable.
    intrusive_fallback: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ClickParams {
    /// The numeric niri window id to click.
    window_id: u64,
    /// Window-relative X coordinate in screenshot/image pixels.
    x: u32,
    /// Window-relative Y coordinate in screenshot/image pixels.
    y: u32,
    /// Optional CUA session id. Reuse the returned session_id for later mouse calls in the same task so the same virtual cursor is moved and reused.
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TypeTextParams {
    /// The numeric niri window id to type into.
    window_id: u64,
    /// Text to type into the window.
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PressKeyParams {
    /// The numeric niri window id to send the key press to.
    window_id: u64,
    /// Key to press, for example Enter, Tab, Escape, Backspace, Delete, ArrowLeft, ArrowRight, ArrowUp, or ArrowDown.
    key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ScrollParams {
    /// The numeric niri window id to scroll.
    window_id: u64,
    /// Scroll direction: up, down, left, or right.
    direction: McpScrollDirection,
    /// Scroll amount in logical compositor units.
    amount: u32,
    /// Optional window-relative X coordinate in screenshot/image pixels.
    x: Option<u32>,
    /// Optional window-relative Y coordinate in screenshot/image pixels.
    y: Option<u32>,
    /// Optional CUA session id. Reuse the returned session_id for later mouse calls in the same task so the same virtual cursor is moved and reused.
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CloseSessionParams {
    /// The CUA session id returned by click or scroll.
    session_id: String,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Workspace {
    id: u64,
    idx: u64,
    name: Option<String>,
    output: Option<String>,
    is_active: bool,
    is_focused: bool,
    active_window_id: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Window {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    pid: Option<i32>,
    #[serde(default)]
    workspace_id: Option<u64>,
    is_focused: bool,
    is_floating: bool,
    is_urgent: bool,
    layout: WindowLayout,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct WindowLayout {
    window_size: [i32; 2],
    tile_size: [f64; 2],
    #[serde(default)]
    tile_pos_in_workspace_view: Option<[f64; 2]>,
    window_offset_in_tile: [f64; 2],
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Output {
    name: String,
    logical: Option<LogicalOutput>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LogicalOutput {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
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

#[derive(Clone)]
struct EnvDefaults {
    niri_socket: Option<PathBuf>,
    xdg_runtime_dir: Option<PathBuf>,
    wayland_display: Option<String>,
}

struct ScrollTarget {
    logical_x: u32,
    logical_y: u32,
    scale: Option<f64>,
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
            None => match env::var(CUA_SESSION_ID_ENV) {
                Ok(session_id) if !session_id.trim().is_empty() => {
                    (normalize_session_id(session_id)?, false)
                }
                _ => (generate_session_id()?, true),
            },
        };
        let cursor_id = format!("{VIRTUAL_CURSOR_ID_PREFIX}-{session_id}");
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

pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    if matches!(cli.command, Commands::Mcp) {
        return run_mcp_server(cli.intrusive_fallback);
    }

    let niri = Niri::discover()?;

    match cli.command {
        Commands::Mcp => unreachable!("MCP mode is handled before niri discovery"),
        Commands::DescribeWorkspace {
            workspace_id,
            screenshots,
        } => {
            let output =
                describe_workspace(&niri, workspace_id, cli.intrusive_fallback, screenshots)?;
            print_json(&output)?;
        }
        Commands::ScreenshotWindow { window_id } => {
            let path =
                screenshot_window(&niri, window_id, &capture_dir()?, cli.intrusive_fallback)?;
            let window = niri.window(window_id)?;
            let scale = niri.window_scale(&window).ok();
            let size = image_size(&path).ok();
            print_json(&ScreenshotOutput {
                window_id,
                path,
                size,
                scale,
                coordinate_space: "screenshot_pixels",
            })?;
        }
        Commands::Click { window_id, x, y } => {
            let (logical_x, logical_y, scale) = niri.screenshot_to_logical(window_id, x, y)?;
            let session = CuaSession::new(None)?;
            niri.virtual_cursor_click(&session.cursor_id, window_id, logical_x, logical_y)?;
            print_json(&serde_json::json!({
                "window_id": window_id,
                "clicked": { "x": x, "y": y },
                "coordinate_space": "screenshot_pixels",
                "sent_logical": { "x": logical_x, "y": logical_y },
                "session_id": session.session_id,
                "session_id_generated": session.generated,
                "session_reuse_hint": session.reuse_hint(),
                "virtual_cursor": session.cursor_id,
                "scale": scale
            }))?;
        }
        Commands::TypeText { window_id, text } => {
            niri.cua_type_text(window_id, &text)?;
            print_json(
                &serde_json::json!({ "window_id": window_id, "typed_chars": text.chars().count() }),
            )?;
        }
        Commands::PressKey { window_id, key } => {
            niri.cua_press_key(window_id, &key)?;
            print_json(&serde_json::json!({ "window_id": window_id, "pressed_key": key }))?;
        }
        Commands::Scroll {
            window_id,
            direction,
            amount,
            x,
            y,
        } => {
            let transformed = niri.scroll_target(window_id, x, y)?;
            let session = CuaSession::new(None)?;
            niri.virtual_cursor_scroll(
                &session.cursor_id,
                window_id,
                direction,
                amount,
                transformed.logical_x,
                transformed.logical_y,
            )?;
            print_json(&serde_json::json!({
                "window_id": window_id,
                "scrolled": amount,
                "coordinate_space": "screenshot_pixels",
                "sent_logical": { "x": transformed.logical_x, "y": transformed.logical_y },
                "session_id": session.session_id,
                "session_id_generated": session.generated,
                "session_reuse_hint": session.reuse_hint(),
                "virtual_cursor": session.cursor_id,
                "scale": transformed.scale
            }))?;
        }
    }

    Ok(())
}

fn run_mcp_server(intrusive_fallback: bool) -> Result<()> {
    tokio::runtime::Runtime::new()?
        .block_on(async move {
            CuaMcpServer { intrusive_fallback }
                .serve(stdio())
                .await?
                .waiting()
                .await?;
            anyhow::Ok(())
        })
        .context("run CUA MCP server")
}

#[derive(Debug, Clone)]
struct CuaMcpServer {
    intrusive_fallback: bool,
}

#[tool_router]
impl CuaMcpServer {
    #[tool(
        name = "describe-workspace",
        description = "Return niri workspace/window metadata. Set include_screenshots=true only when a composite screenshot is needed."
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
        description = "Capture a single niri window and return the PNG image directly."
    )]
    fn view_window(&self, Parameters(params): Parameters<ViewWindowParams>) -> CallToolResult {
        self.view_window_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "click",
        description = "Click inside a window at window-relative screenshot pixel coordinates."
    )]
    fn click(&self, Parameters(params): Parameters<ClickParams>) -> CallToolResult {
        self.click_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(name = "type-text", description = "Type text into a window.")]
    fn type_text(&self, Parameters(params): Parameters<TypeTextParams>) -> CallToolResult {
        self.type_text_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "press-key",
        description = "Press one named key in a window, for example Enter, Tab, Escape, Backspace, Delete, or ArrowLeft."
    )]
    fn press_key(&self, Parameters(params): Parameters<PressKeyParams>) -> CallToolResult {
        self.press_key_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "scroll",
        description = "Scroll inside a window, optionally at window-relative screenshot pixel coordinates."
    )]
    fn scroll(&self, Parameters(params): Parameters<ScrollParams>) -> CallToolResult {
        self.scroll_inner(params).unwrap_or_else(tool_error)
    }

    #[tool(
        name = "close-session",
        description = "Close a CUA mouse session and destroy its associated virtual cursor."
    )]
    fn close_session(&self, Parameters(params): Parameters<CloseSessionParams>) -> CallToolResult {
        self.close_session_inner(params).unwrap_or_else(tool_error)
    }
}

#[tool_handler(name = "tic-cua", version = "0.1.0")]
impl ServerHandler for CuaMcpServer {}

impl CuaMcpServer {
    fn intrusive_fallback(&self, override_value: Option<bool>) -> bool {
        override_value.unwrap_or(self.intrusive_fallback)
    }

    fn describe_workspace_inner(&self, params: DescribeWorkspaceParams) -> Result<CallToolResult> {
        let niri = Niri::discover()?;
        let output = describe_workspace(
            &niri,
            params.workspace_id,
            self.intrusive_fallback(params.intrusive_fallback),
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
            self.intrusive_fallback(params.intrusive_fallback),
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

    fn click_inner(&self, params: ClickParams) -> Result<CallToolResult> {
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

    fn type_text_inner(&self, params: TypeTextParams) -> Result<CallToolResult> {
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

    fn press_key_inner(&self, params: PressKeyParams) -> Result<CallToolResult> {
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

    fn scroll_inner(&self, params: ScrollParams) -> Result<CallToolResult> {
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

    fn close_session_inner(&self, params: CloseSessionParams) -> Result<CallToolResult> {
        let session = CuaSession::new(Some(params.session_id))?;
        let niri = Niri::discover()?;
        niri.destroy_virtual_cursor(&session.cursor_id)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({
                "session_id": session.session_id,
                "virtual_cursor": session.cursor_id,
                "closed": true
            })
            .to_string(),
        )]))
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

fn image_content(path: &Path) -> Result<Content> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(Content::image(
        base64::engine::general_purpose::STANDARD.encode(data),
        "image/png",
    ))
}

fn tool_error(err: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![Content::text(error_chain(&err))])
}

fn describe_workspace(
    niri: &Niri,
    workspace_id: Option<u64>,
    _intrusive_fallback: bool,
    include_screenshots: bool,
) -> Result<DescribeWorkspaceOutput> {
    let originally_focused_window = niri.focused_window().ok().map(|window| window.id);
    let workspaces = niri.workspaces()?;
    let id = workspace_id
        .or_else(env_workspace_id)
        .or_else(|| workspaces.iter().find(|w| w.is_focused).map(|w| w.id))
        .or_else(|| workspaces.iter().find(|w| w.is_active).map(|w| w.id))
        .ok_or_else(|| {
            anyhow!("no workspace id was provided and niri did not report an active workspace")
        })?;

    let workspace = workspaces
        .iter()
        .find(|w| w.id == id || w.idx == id)
        .cloned()
        .ok_or_else(|| anyhow!("workspace {id} was not found by id or idx"))?;
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
        niri.cua_screenshot_workspace(workspace.id, &path, false)?;
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
    match niri.cua_screenshot_window(window_id, &path, false) {
        Ok(()) => return Ok(path),
        Err(err) if !intrusive_fallback => {
            return Err(err).with_context(|| {
                "CUA-aligned niri screenshot failed; pass --intrusive-fallback to use grim after focusing the window"
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
            .ok_or_else(|| anyhow!("non-utf8 screenshot path"))?,
    )?;
    Ok(())
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
            anyhow!(
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
            anyhow!(
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

fn screenshot_coord_to_logical(coord: u32, scale: f64) -> Result<u32> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(anyhow!("invalid output scale {scale}"));
    }
    Ok((f64::from(coord) / scale).round().max(0.0) as u32)
}

fn capture_dir() -> Result<PathBuf> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let dir = env::temp_dir().join(format!("cua-{}-{now}", std::process::id()));
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

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

struct Niri {
    env: EnvDefaults,
}

impl Niri {
    fn discover() -> Result<Self> {
        Ok(Self {
            env: EnvDefaults::discover()?,
        })
    }

    fn windows(&self) -> Result<Vec<Window>> {
        self.msg_json(["--json", "windows"])
    }

    fn workspaces(&self) -> Result<Vec<Workspace>> {
        self.msg_json(["--json", "workspaces"])
    }

    fn outputs(&self) -> Result<Vec<Output>> {
        let value: serde_json::Value = self.msg_json(["--json", "outputs"])?;
        if value.is_array() {
            serde_json::from_value(value).context("parse niri outputs array")
        } else {
            let outputs: HashMap<String, Output> =
                serde_json::from_value(value).context("parse niri outputs map")?;
            Ok(outputs.into_values().collect())
        }
    }

    fn window(&self, window_id: u64) -> Result<Window> {
        self.windows()?
            .into_iter()
            .find(|window| window.id == window_id)
            .ok_or_else(|| anyhow!("niri reports no window with id {window_id}"))
    }

    fn focused_window(&self) -> Result<Window> {
        self.msg_json::<Option<Window>, _, _>(["--json", "focused-window"])?
            .ok_or_else(|| anyhow!("niri reports no focused window"))
    }

    fn logical_output_for_window(&self, window: &Window) -> Result<LogicalOutput> {
        let workspace_id = window
            .workspace_id
            .ok_or_else(|| anyhow!("window {} is not on a workspace", window.id))?;
        let workspace = self
            .workspaces()?
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or_else(|| {
                anyhow!(
                    "workspace {workspace_id} for window {} was not found",
                    window.id
                )
            })?;
        let output_name = workspace
            .output
            .ok_or_else(|| anyhow!("workspace {workspace_id} has no output"))?;
        let output = self
            .outputs()?
            .into_iter()
            .find(|output| output.name == output_name)
            .ok_or_else(|| {
                anyhow!("output {output_name} for workspace {workspace_id} was not found")
            })?;
        output
            .logical
            .ok_or_else(|| anyhow!("output {output_name} has no logical mapping"))
    }

    fn window_scale(&self, window: &Window) -> Result<f64> {
        Ok(self.logical_output_for_window(window)?.scale)
    }

    fn screenshot_to_logical(&self, window_id: u64, x: u32, y: u32) -> Result<(u32, u32, f64)> {
        let window = self.window(window_id)?;
        let scale = self.window_scale(&window)?;
        Ok((
            screenshot_coord_to_logical(x, scale)?,
            screenshot_coord_to_logical(y, scale)?,
            scale,
        ))
    }

    fn scroll_target(
        &self,
        window_id: u64,
        x: Option<u32>,
        y: Option<u32>,
    ) -> Result<ScrollTarget> {
        let window = self.window(window_id)?;
        let scale = self.window_scale(&window)?;
        let center_x = (f64::from(window.layout.window_size[0]) / 2.0).max(0.0);
        let center_y = (f64::from(window.layout.window_size[1]) / 2.0).max(0.0);

        Ok(ScrollTarget {
            logical_x: x
                .map(|coord| screenshot_coord_to_logical(coord, scale))
                .transpose()?
                .unwrap_or_else(|| center_x.round() as u32),
            logical_y: y
                .map(|coord| screenshot_coord_to_logical(coord, scale))
                .transpose()?
                .unwrap_or_else(|| center_y.round() as u32),
            scale: if x.is_some() || y.is_some() {
                Some(scale)
            } else {
                None
            },
        })
    }

    fn focus_window(&self, window_id: u64) -> Result<()> {
        self.msg(["action", "focus-window", "--id", &window_id.to_string()])
    }

    fn ensure_virtual_cursor(&self, cursor_id: &str, window_id: u64, x: u32, y: u32) -> Result<()> {
        let x = x.to_string();
        let y = y.to_string();
        let window_id = window_id.to_string();
        let cursor_theme = virtual_cursor_theme();
        let cursor_shape = virtual_cursor_shape();
        let cursor_color = virtual_cursor_color();
        let cursor_outline_color = virtual_cursor_outline_color();

        let mut update_args = vec![
            "update-virtual-cursor",
            "--cursor-id",
            cursor_id,
            "--window-id",
            &window_id,
            "--x",
            &x,
            "--y",
            &y,
        ];
        if let Some(shape) = cursor_shape.as_deref() {
            update_args.extend(["--shape", shape]);
            if let Some(color) = cursor_color.as_deref() {
                update_args.extend(["--color", color]);
            }
            if let Some(outline_color) = cursor_outline_color.as_deref() {
                update_args.extend(["--outline-color", outline_color]);
            }
        } else {
            update_args.extend([
                "--cursor-theme",
                cursor_theme.as_str(),
                "--cursor-icon",
                VIRTUAL_CURSOR_ICON,
            ]);
        }
        let size = VIRTUAL_CURSOR_SIZE.to_string();
        let duration = VIRTUAL_CURSOR_MOVE_MS.to_string();
        update_args.extend([
            "--size",
            &size,
            "--duration-ms",
            &duration,
            "--visible",
            "true",
            "--z-index",
            "100",
        ]);
        let update = self.msg(update_args);
        if update.is_ok() {
            return Ok(());
        }

        let mut create_args = vec![
            "create-virtual-cursor",
            "--cursor-id",
            cursor_id,
            "--window-id",
            &window_id,
            "--x",
            &x,
            "--y",
            &y,
        ];
        if let Some(shape) = cursor_shape.as_deref() {
            create_args.extend(["--shape", shape]);
            if let Some(color) = cursor_color.as_deref() {
                create_args.extend(["--color", color]);
            }
            if let Some(outline_color) = cursor_outline_color.as_deref() {
                create_args.extend(["--outline-color", outline_color]);
            }
        } else {
            create_args.extend([
                "--cursor-theme",
                cursor_theme.as_str(),
                "--cursor-icon",
                VIRTUAL_CURSOR_ICON,
            ]);
        }
        create_args.extend([
            "--size",
            &size,
            "--duration-ms",
            &duration,
            "--replace-existing",
        ]);
        self.msg(create_args)
    }

    fn virtual_cursor_click(&self, cursor_id: &str, window_id: u64, x: u32, y: u32) -> Result<()> {
        self.ensure_virtual_cursor(cursor_id, window_id, x, y)?;
        self.msg(["action", "virtual-cursor-click", "--cursor-id", cursor_id])
    }

    fn virtual_cursor_scroll(
        &self,
        cursor_id: &str,
        window_id: u64,
        direction: ScrollDirection,
        amount: u32,
        x: u32,
        y: u32,
    ) -> Result<()> {
        let amount = i32::try_from(amount).context("scroll amount exceeds i32")?;
        let (scroll_x, scroll_y) = match direction {
            ScrollDirection::Up => (0, -amount),
            ScrollDirection::Down => (0, amount),
            ScrollDirection::Left => (-amount, 0),
            ScrollDirection::Right => (amount, 0),
        };

        self.ensure_virtual_cursor(cursor_id, window_id, x, y)?;
        self.msg([
            "action",
            "virtual-cursor-scroll",
            "--cursor-id",
            cursor_id,
            "--scroll-x",
            &scroll_x.to_string(),
            "--scroll-y",
            &scroll_y.to_string(),
        ])
    }

    fn destroy_virtual_cursor(&self, cursor_id: &str) -> Result<()> {
        self.msg(["destroy-virtual-cursor", "--cursor-id", cursor_id])
    }

    fn cua_type_text(&self, window_id: u64, text: &str) -> Result<()> {
        self.msg([
            "action",
            "cua-type-text",
            "--id",
            &window_id.to_string(),
            "--text",
            text,
        ])
    }

    fn cua_press_key(&self, window_id: u64, key: &str) -> Result<()> {
        self.msg([
            "action",
            "cua-press-key",
            "--id",
            &window_id.to_string(),
            "--key",
            key,
        ])
    }

    fn cua_screenshot_window(&self, window_id: u64, path: &Path, notify: bool) -> Result<()> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 screenshot path"))?;
        self.msg([
            "action",
            "cua-screenshot-window",
            "--id",
            &window_id.to_string(),
            "--write-to-disk",
            "true",
            "--notify",
            if notify { "true" } else { "false" },
            "--path",
            path,
        ])?;
        wait_for_file(Path::new(path), Duration::from_secs(5))
    }

    fn cua_screenshot_workspace(&self, workspace_id: u64, path: &Path, notify: bool) -> Result<()> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 screenshot path"))?;
        self.msg([
            "action",
            "cua-screenshot-workspace",
            "--id",
            &workspace_id.to_string(),
            "--write-to-disk",
            "true",
            "--notify",
            if notify { "true" } else { "false" },
            "--path",
            path,
        ])?;
        wait_for_file(Path::new(path), Duration::from_secs(5))
    }

    fn grim(&self, geometry: &str, path: &str) -> Result<()> {
        let mut command = Command::new("grim");
        self.env.apply(&mut command);
        let output = command
            .args(["-g", geometry, path])
            .stdin(Stdio::null())
            .output()
            .context("run grim")?;
        if !output.status.success() {
            return Err(anyhow!(
                "grim failed: {}\nstdout: {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    fn msg_json<T, I, S>(&self, args: I) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let output = self.msg_output(args)?;
        serde_json::from_slice(&output.stdout).context("parse niri JSON")
    }

    fn msg<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.msg_output(args).map(|_| ())
    }

    fn msg_output<I, S>(&self, args: I) -> Result<std::process::Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut command = Command::new("niri");
        self.env.apply(&mut command);
        command.arg("msg");
        for arg in args {
            command.arg(arg.as_ref());
        }

        let output = command
            .stdin(Stdio::null())
            .output()
            .context("run niri msg")?;
        if !output.status.success() {
            return Err(anyhow!(
                "niri msg failed: {}\nstdout: {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(output)
    }
}

fn wait_for_file(path: &Path, timeout: Duration) -> Result<()> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if path.metadata().is_ok_and(|metadata| metadata.len() > 0) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(40));
    }
    Err(anyhow!("timed out waiting for {}", path.display()))
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
        return Err(anyhow!("session_id must not be empty"));
    }
    if !session_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(anyhow!(
            "session_id may only contain ASCII letters, digits, '-' and '_'"
        ));
    }
    Ok(session_id.to_owned())
}

fn virtual_cursor_theme() -> String {
    env::var(CUA_CURSOR_THEME_ENV)
        .ok()
        .map(|theme| theme.trim().to_owned())
        .filter(|theme| !theme.is_empty())
        .unwrap_or_else(|| VIRTUAL_CURSOR_THEME.to_owned())
}

fn virtual_cursor_shape() -> Option<String> {
    env::var(CUA_CURSOR_SHAPE_ENV)
        .ok()
        .map(|shape| shape.trim().to_owned())
        .filter(|shape| !shape.is_empty())
}

fn virtual_cursor_color() -> Option<String> {
    env::var(CUA_CURSOR_COLOR_ENV)
        .ok()
        .map(|color| color.trim().to_owned())
        .filter(|color| !color.is_empty())
}

fn virtual_cursor_outline_color() -> Option<String> {
    env::var(CUA_CURSOR_OUTLINE_COLOR_ENV)
        .ok()
        .map(|color| color.trim().to_owned())
        .filter(|color| !color.is_empty())
}

impl EnvDefaults {
    fn discover() -> Result<Self> {
        let explicit_runtime_dir = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let current_user_runtime_dir = uid()
            .ok()
            .map(|uid| PathBuf::from(format!("/run/user/{uid}")));

        let niri_socket = env::var_os("NIRI_SOCKET")
            .map(PathBuf::from)
            .or_else(|| {
                explicit_runtime_dir
                    .as_ref()
                    .and_then(|dir| latest_matching(dir, "niri.", ".sock"))
            })
            .or_else(|| {
                current_user_runtime_dir
                    .as_ref()
                    .and_then(|dir| latest_matching(dir, "niri.", ".sock"))
            })
            .or_else(find_niri_socket);

        let xdg_runtime_dir = explicit_runtime_dir
            .or_else(|| {
                niri_socket
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .or(current_user_runtime_dir);

        let wayland_display = env::var("WAYLAND_DISPLAY")
            .ok()
            .or_else(|| {
                niri_socket
                    .as_ref()
                    .and_then(|path| infer_wayland_display(path))
            })
            .or_else(|| {
                xdg_runtime_dir
                    .as_ref()
                    .and_then(|dir| find_wayland_display(dir))
            });

        Ok(Self {
            niri_socket,
            xdg_runtime_dir,
            wayland_display,
        })
    }

    fn apply(&self, command: &mut Command) {
        if let Some(path) = &self.niri_socket {
            command.env("NIRI_SOCKET", path);
        }
        if let Some(path) = &self.xdg_runtime_dir {
            command.env("XDG_RUNTIME_DIR", path);
        }
        if let Some(display) = &self.wayland_display {
            command.env("WAYLAND_DISPLAY", display);
        }
    }
}

fn uid() -> Result<String> {
    if let Ok(uid) = env::var("UID") {
        return Ok(uid);
    }
    let output = Command::new("id").arg("-u").output().context("run id -u")?;
    if !output.status.success() {
        return Err(anyhow!("id -u failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn latest_matching(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
}

fn find_niri_socket() -> Option<PathBuf> {
    fs::read_dir("/run/user")
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| latest_matching(&entry.path(), "niri.", ".sock"))
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
}

fn infer_wayland_display(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("niri.")?.strip_suffix(".sock")?;
    let (display, _) = rest.rsplit_once('.')?;
    Some(display.to_string())
}

fn find_wayland_display(dir: &Path) -> Option<String> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("wayland-") && !name.ends_with(".lock"))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tiri_optional_ipc_shapes() {
        let focused_window: Option<Window> = serde_json::from_str("null").unwrap();
        assert!(focused_window.is_none());

        let window: Window = serde_json::from_str(
            r#"{
                "id": 7,
                "title": null,
                "app_id": null,
                "pid": null,
                "workspace_id": null,
                "is_focused": false,
                "is_floating": false,
                "is_urgent": false,
                "layout": {
                    "pos_in_scrolling_layout": [1, 1],
                    "tile_size": [640.0, 480.0],
                    "window_size": [640, 480],
                    "tile_pos_in_workspace_view": null,
                    "window_offset_in_tile": [0.0, 0.0]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(window.title, None);
        assert_eq!(window.pid, None);
        assert_eq!(window_width(&window).unwrap(), 640);

        let output: Output = serde_json::from_str(
            r#"{
                "name": "HEADLESS-1",
                "logical": null
            }"#,
        )
        .unwrap();
        assert!(output.logical.is_none());
    }

    #[test]
    fn parses_tiri_outputs_map_shape() {
        let outputs: HashMap<String, Output> = serde_json::from_str(
            r#"{
                "eDP-1": {
                    "name": "eDP-1",
                    "logical": {
                        "x": 0,
                        "y": 0,
                        "width": 1680,
                        "height": 1120,
                        "scale": 1.5
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(outputs["eDP-1"].logical.as_ref().unwrap().scale, 1.5);
    }

    #[test]
    fn converts_screenshot_pixels_to_logical_coordinates() {
        assert_eq!(screenshot_coord_to_logical(0, 1.5).unwrap(), 0);
        assert_eq!(screenshot_coord_to_logical(148, 1.5).unwrap(), 99);
        assert_eq!(screenshot_coord_to_logical(270, 1.5).unwrap(), 180);
        assert_eq!(screenshot_coord_to_logical(410, 1.5).unwrap(), 273);
    }

    #[test]
    fn rejects_invalid_coordinate_scale() {
        assert!(screenshot_coord_to_logical(10, 0.0).is_err());
        assert!(screenshot_coord_to_logical(10, f64::NAN).is_err());
    }
}
