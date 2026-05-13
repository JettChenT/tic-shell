use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use image::{GenericImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    #[command(alias = "describe_workspace")]
    DescribeWorkspace {
        workspace_id: Option<u64>,
    },
    #[command(alias = "screenshot_window")]
    ScreenshotWindow {
        window_id: u64,
    },
    Click {
        window_id: u64,
        x: u32,
        y: u32,
    },
    #[command(alias = "type")]
    TypeText {
        window_id: u64,
        text: String,
    },
    Scroll {
        window_id: u64,
        direction: ScrollDirection,
        amount: u32,
        x: Option<u32>,
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
    screenshot_dir: PathBuf,
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
}

#[derive(Debug, Serialize)]
struct ScreenshotOutput {
    window_id: u64,
    path: PathBuf,
}

#[derive(Clone)]
struct EnvDefaults {
    niri_socket: Option<PathBuf>,
    xdg_runtime_dir: Option<PathBuf>,
    wayland_display: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let niri = Niri::discover()?;

    match cli.command {
        Commands::DescribeWorkspace { workspace_id } => {
            let output = describe_workspace(&niri, workspace_id, cli.intrusive_fallback)?;
            print_json(&output)?;
        }
        Commands::ScreenshotWindow { window_id } => {
            let path =
                screenshot_window(&niri, window_id, &capture_dir()?, cli.intrusive_fallback)?;
            print_json(&ScreenshotOutput { window_id, path })?;
        }
        Commands::Click { window_id, x, y } => {
            niri.cua_click(window_id, x, y)?;
            print_json(
                &serde_json::json!({ "window_id": window_id, "clicked": { "x": x, "y": y } }),
            )?;
        }
        Commands::TypeText { window_id, text } => {
            niri.cua_type_text(window_id, &text)?;
            print_json(
                &serde_json::json!({ "window_id": window_id, "typed_chars": text.chars().count() }),
            )?;
        }
        Commands::Scroll {
            window_id,
            direction,
            amount,
            x,
            y,
        } => {
            niri.cua_scroll(window_id, direction, amount, x, y)?;
            print_json(&serde_json::json!({ "window_id": window_id, "scrolled": amount }))?;
        }
    }

    Ok(())
}

fn describe_workspace(
    niri: &Niri,
    workspace_id: Option<u64>,
    intrusive_fallback: bool,
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
        .into_iter()
        .find(|w| w.id == id || w.idx == id)
        .ok_or_else(|| anyhow!("workspace {id} was not found by id or idx"))?;

    let dir = capture_dir()?;
    let windows: Vec<Window> = niri
        .windows()?
        .into_iter()
        .filter(|window| window.workspace_id == Some(workspace.id))
        .collect();

    let mut infos = Vec::with_capacity(windows.len());
    let mut screenshots = Vec::new();
    for window in windows {
        let screenshot_result = screenshot_window(niri, window.id, &dir, intrusive_fallback);
        let (screenshot, screenshot_error) = match screenshot_result {
            Ok(path) => (Some(path), None),
            Err(err) => (None, Some(err.to_string())),
        };
        if let Some(path) = &screenshot {
            screenshots.push(path.clone());
        }
        infos.push(WindowInfo {
            id: window.id,
            title: window.title.unwrap_or_else(|| "(untitled)".to_string()),
            app_id: window.app_id.unwrap_or_default(),
            pid: window.pid,
            is_focused: window.is_focused,
            is_floating: window.is_floating,
            screenshot,
            screenshot_error,
            size: window.layout.window_size,
        });
    }

    let composite_screenshot = if screenshots.is_empty() {
        None
    } else {
        let path = dir.join(format!("workspace-{}-composite.png", workspace.id));
        make_composite(&screenshots, &path)?;
        Some(path)
    };

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
    match niri.screenshot_window(window_id, &path, false) {
        Ok(()) => return Ok(path),
        Err(err) if !intrusive_fallback => {
            return Err(err).with_context(|| {
                "non-intrusive niri screenshot failed; pass --intrusive-fallback to use grim after focusing the window"
            });
        }
        Err(_) => {
            let _ = fs::remove_file(&path);
        }
    }

    let (x, y, width, height) = focus_and_resolve_rect(niri, window_id)?;
    niri.grim(
        &format!("{x},{y} {width}x{height}"),
        path.to_str()
            .ok_or_else(|| anyhow!("non-utf8 screenshot path"))?,
    )?;
    Ok(path)
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

fn make_composite(paths: &[PathBuf], out: &Path) -> Result<()> {
    let mut images = Vec::new();
    for path in paths {
        images.push(
            image::open(path)
                .with_context(|| format!("open {}", path.display()))?
                .to_rgba8(),
        );
    }

    let columns = cmp::min(2, images.len() as u32);
    let rows = (images.len() as u32).div_ceil(columns);
    let max_w = images.iter().map(|img| img.width()).max().unwrap_or(1);
    let max_h = images.iter().map(|img| img.height()).max().unwrap_or(1);
    let gap = 16;
    let width = columns * max_w + (columns + 1) * gap;
    let height = rows * max_h + (rows + 1) * gap;
    let mut canvas: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(width, height, Rgba([24, 24, 24, 255]));

    for (idx, image) in images.iter().enumerate() {
        let col = idx as u32 % columns;
        let row = idx as u32 / columns;
        let x = gap + col * (max_w + gap);
        let y = gap + row * (max_h + gap);
        canvas.copy_from(image, x, y)?;
    }

    canvas
        .save(out)
        .with_context(|| format!("save {}", out.display()))?;
    Ok(())
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
        self.msg_json(["--json", "outputs"])
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

    fn focus_window(&self, window_id: u64) -> Result<()> {
        self.msg(["action", "focus-window", "--id", &window_id.to_string()])
    }

    fn cua_click(&self, window_id: u64, x: u32, y: u32) -> Result<()> {
        self.msg([
            "action",
            "cua-click",
            "--id",
            &window_id.to_string(),
            "--x",
            &x.to_string(),
            "--y",
            &y.to_string(),
        ])
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

    fn cua_scroll(
        &self,
        window_id: u64,
        direction: ScrollDirection,
        amount: u32,
        x: Option<u32>,
        y: Option<u32>,
    ) -> Result<()> {
        let amount = i32::try_from(amount).context("scroll amount exceeds i32")?;
        let (scroll_x, scroll_y) = match direction {
            ScrollDirection::Up => (0, -amount),
            ScrollDirection::Down => (0, amount),
            ScrollDirection::Left => (-amount, 0),
            ScrollDirection::Right => (amount, 0),
        };

        let mut args = vec![
            "action".to_string(),
            "cua-scroll".to_string(),
            "--id".to_string(),
            window_id.to_string(),
            "--scroll-x".to_string(),
            scroll_x.to_string(),
            "--scroll-y".to_string(),
            scroll_y.to_string(),
        ];
        if let Some(x) = x {
            args.push("--x".to_string());
            args.push(x.to_string());
        }
        if let Some(y) = y {
            args.push("--y".to_string());
            args.push(y.to_string());
        }

        self.msg(args)
    }

    fn screenshot_window(&self, window_id: u64, path: &Path, notify: bool) -> Result<()> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 screenshot path"))?;
        self.msg([
            "action",
            "screenshot-window",
            "--id",
            &window_id.to_string(),
            "--write-to-disk",
            "true",
            "--show-pointer",
            "false",
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

        let output = command.output().context("run niri msg")?;
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
}
