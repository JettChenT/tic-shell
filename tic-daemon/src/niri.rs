use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: u64,
    pub idx: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    pub is_active: bool,
    pub is_focused: bool,
    #[serde(default)]
    pub active_window_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Window {
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub pid: Option<i32>,
    #[serde(default)]
    pub workspace_id: Option<u64>,
    #[serde(default)]
    pub is_focused: bool,
    #[serde(default)]
    pub is_floating: bool,
    #[serde(default)]
    pub is_urgent: bool,
    pub layout: WindowLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowLayout {
    pub window_size: [i32; 2],
    pub tile_size: [f64; 2],
    #[serde(default)]
    pub tile_pos_in_workspace_view: Option<[f64; 2]>,
    pub window_offset_in_tile: [f64; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Output {
    pub name: String,
    #[serde(default)]
    pub logical: Option<LogicalOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogicalOutput {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

pub struct ScrollTarget {
    pub logical_x: u32,
    pub logical_y: u32,
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TiriEvent {
    WorkspacesChanged {
        workspaces: Vec<Workspace>,
    },
    WorkspaceActivated {
        id: u64,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace_id: u64,
        active_window_id: Option<u64>,
    },
    WindowsChanged {
        windows: Vec<Window>,
    },
    WindowOpenedOrChanged {
        window: Window,
    },
    WindowClosed {
        id: u64,
    },
    WindowFocusChanged {
        id: Option<u64>,
    },
    WindowLayoutsChanged {
        changes: Vec<(u64, WindowLayout)>,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone)]
pub struct Niri {
    env: EnvDefaults,
}

#[derive(Clone)]
struct EnvDefaults {
    niri_socket: Option<PathBuf>,
    xdg_runtime_dir: Option<PathBuf>,
    wayland_display: Option<String>,
}

impl Niri {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            env: EnvDefaults::discover()?,
        })
    }

    pub fn windows(&self) -> Result<Vec<Window>> {
        self.msg_json(["--json", "windows"])
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        self.msg_json(["--json", "workspaces"])
    }

    pub fn outputs(&self) -> Result<Vec<Output>> {
        let value: serde_json::Value = self.msg_json(["--json", "outputs"])?;
        if value.is_array() {
            serde_json::from_value(value).context("parse niri outputs array")
        } else {
            let outputs: HashMap<String, Output> =
                serde_json::from_value(value).context("parse niri outputs map")?;
            Ok(outputs.into_values().collect())
        }
    }

    pub fn window(&self, window_id: u64) -> Result<Window> {
        self.windows()?
            .into_iter()
            .find(|window| window.id == window_id)
            .ok_or_else(|| anyhow!("niri reports no window with id {window_id}"))
    }

    pub fn focused_window(&self) -> Result<Window> {
        self.msg_json::<Option<Window>, _, _>(["--json", "focused-window"])?
            .ok_or_else(|| anyhow!("niri reports no focused window"))
    }

    pub fn logical_output_for_window(&self, window: &Window) -> Result<LogicalOutput> {
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

    pub fn window_scale(&self, window: &Window) -> Result<f64> {
        Ok(self.logical_output_for_window(window)?.scale)
    }

    pub fn screenshot_to_logical(&self, window_id: u64, x: u32, y: u32) -> Result<(u32, u32, f64)> {
        let window = self.window(window_id)?;
        let scale = self.window_scale(&window)?;
        Ok((
            screenshot_coord_to_logical(x, scale)?,
            screenshot_coord_to_logical(y, scale)?,
            scale,
        ))
    }

    pub fn scroll_target(
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

    pub fn focus_window(&self, window_id: u64) -> Result<()> {
        self.msg(["action", "focus-window", "--id", &window_id.to_string()])
    }

    pub fn screenshot_window(&self, window_id: u64, path: &Path) -> Result<()> {
        let path_str = path
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
            "false",
            "--path",
            path_str,
        ])?;
        wait_for_file(path, Duration::from_secs(5))
    }

    pub fn screenshot_workspace(&self, workspace_id: u64, path: &Path) -> Result<()> {
        let path_str = path
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
            "false",
            "--path",
            path_str,
        ])?;
        wait_for_file(path, Duration::from_secs(5))
    }

    pub fn grim(&self, geometry: &str, path: &str) -> Result<()> {
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

    pub fn destroy_virtual_cursor(&self, cursor_id: &str) -> Result<()> {
        self.msg(["destroy-virtual-cursor", "--cursor-id", cursor_id])
    }

    pub fn create_virtual_cursor(
        &self,
        cursor_id: &str,
        window_id: u64,
        x: u32,
        y: u32,
        cursor_theme: &str,
    ) -> Result<()> {
        self.msg([
            "create-virtual-cursor",
            "--cursor-id",
            cursor_id,
            "--window-id",
            &window_id.to_string(),
            "--x",
            &x.to_string(),
            "--y",
            &y.to_string(),
            "--cursor-theme",
            cursor_theme,
            "--cursor-icon",
            "left_ptr",
            "--size",
            "32",
            "--duration-ms",
            "0",
            "--replace-existing",
        ])
    }

    pub fn create_virtual_cursor_at_pointer(
        &self,
        cursor_id: &str,
        window_id: u64,
        cursor_theme: &str,
    ) -> Result<()> {
        self.msg([
            "create-virtual-cursor",
            "--cursor-id",
            cursor_id,
            "--window-id",
            &window_id.to_string(),
            "--x",
            "0",
            "--y",
            "0",
            "--at-pointer",
            "--cursor-theme",
            cursor_theme,
            "--cursor-icon",
            "left_ptr",
            "--size",
            "32",
            "--duration-ms",
            "0",
            "--replace-existing",
        ])
    }

    pub fn set_hardware_cursor(&self, cursor_theme: &str) -> Result<()> {
        self.msg([
            "set-hardware-cursor",
            "--cursor-theme",
            cursor_theme,
            "--cursor-icon",
            "left_ptr",
            "--size",
            "32",
        ])
    }

    pub fn clear_hardware_cursor(&self) -> Result<()> {
        self.msg(["clear-hardware-cursor"])
    }

    pub fn virtual_cursor_click(
        &self,
        cursor_id: &str,
        window_id: u64,
        x: u32,
        y: u32,
    ) -> Result<()> {
        self.create_virtual_cursor(cursor_id, window_id, x, y, &virtual_cursor_theme())?;
        self.msg(["action", "virtual-cursor-click", "--cursor-id", cursor_id])
    }

    pub fn virtual_cursor_scroll(
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
        self.create_virtual_cursor(cursor_id, window_id, x, y, &virtual_cursor_theme())?;
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

    pub fn cua_type_text(&self, window_id: u64, text: &str) -> Result<()> {
        self.msg([
            "action",
            "cua-type-text",
            "--id",
            &window_id.to_string(),
            "--text",
            text,
        ])
    }

    pub fn cua_press_key(&self, window_id: u64, key: &str) -> Result<()> {
        self.msg([
            "action",
            "cua-press-key",
            "--id",
            &window_id.to_string(),
            "--key",
            key,
        ])
    }

    pub fn event_stream(&self) -> Result<EventReader> {
        let socket = self
            .env
            .niri_socket
            .clone()
            .ok_or_else(|| anyhow!("NIRI_SOCKET was not discovered"))?;
        let mut stream = UnixStream::connect(&socket)
            .with_context(|| format!("connect to niri socket {}", socket.display()))?;
        stream.write_all(br#""EventStream""#)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader.read_line(&mut first)?;
        let reply: serde_json::Value =
            serde_json::from_str(first.trim()).context("parse event-stream reply")?;
        if reply != serde_json::json!({"Ok":"Handled"}) && reply != serde_json::json!("Handled") {
            // Newer niri encodes Reply::Ok(Response::Handled) as {"Ok":"Handled"}.
            if reply.get("Err").is_some() {
                return Err(anyhow!("event-stream failed: {reply}"));
            }
        }
        Ok(EventReader { reader })
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

fn screenshot_coord_to_logical(coord: u32, scale: f64) -> Result<u32> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(anyhow!("invalid output scale {scale}"));
    }
    Ok((f64::from(coord) / scale).round().max(0.0) as u32)
}

fn wait_for_file(path: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if path.metadata().is_ok_and(|metadata| metadata.len() > 0) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(40));
    }
    Err(anyhow!("timed out waiting for {}", path.display()))
}

fn virtual_cursor_theme() -> String {
    std::env::var("CUA_CURSOR_THEME")
        .ok()
        .map(|theme| theme.trim().to_owned())
        .filter(|theme| !theme.is_empty())
        .unwrap_or_else(|| "Tiri-CUA".to_string())
}

pub fn list_cursor_themes() -> Vec<String> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/jettc"));
    let roots = [
        home.join(".local/share/icons"),
        home.join("dev/tiri/resources/cursors"),
    ];
    let mut themes = roots
        .iter()
        .flat_map(|root| {
            fs::read_dir(root)
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
        })
        .filter_map(|entry| {
            let ty = entry.file_type().ok()?;
            let name = entry.file_name().into_string().ok()?;
            (ty.is_dir() && name.starts_with("Tiri-CUA")).then_some(name)
        })
        .collect::<Vec<_>>();
    themes.sort();
    themes.dedup();
    if themes.is_empty() {
        vec!["Tiri-CUA".to_string()]
    } else {
        themes
    }
}

pub struct EventReader {
    reader: BufReader<UnixStream>,
}

impl EventReader {
    pub fn read_event(&mut self) -> Result<TiriEvent> {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line)?;
        if read == 0 {
            return Err(anyhow!("niri event stream closed"));
        }
        serde_json::from_str(line.trim()).context("parse niri event")
    }
}

impl EnvDefaults {
    fn discover() -> Result<Self> {
        let explicit_runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let current_user_runtime_dir = uid()
            .ok()
            .map(|uid| PathBuf::from(format!("/run/user/{uid}")));

        let niri_socket = std::env::var_os("NIRI_SOCKET")
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
            });

        let xdg_runtime_dir = explicit_runtime_dir
            .or_else(|| {
                niri_socket
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .or(current_user_runtime_dir);
        let wayland_display = std::env::var("WAYLAND_DISPLAY").ok().or_else(|| {
            niri_socket
                .as_ref()
                .and_then(|path| infer_wayland_display(path))
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
    let output = Command::new("id").arg("-u").output().context("run id -u")?;
    if !output.status.success() {
        return Err(anyhow!("id -u failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn latest_matching(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    fs_read_dir(dir)
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
}

fn fs_read_dir(dir: &Path) -> Box<dyn Iterator<Item = std::io::Result<std::fs::DirEntry>>> {
    match std::fs::read_dir(dir) {
        Ok(read_dir) => Box::new(read_dir),
        Err(_) => Box::new(std::iter::empty()),
    }
}

fn infer_wayland_display(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("niri.")?.strip_suffix(".sock")?;
    let (display, _) = rest.rsplit_once('.')?;
    Some(display.to_string())
}

pub fn windows_by_id(windows: &[Window]) -> HashMap<u64, Window> {
    windows
        .iter()
        .map(|window| (window.id, window.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_externally_tagged_event_stream_shape() {
        let event: TiriEvent = serde_json::from_str(r#"{"WindowClosed":{"id":42}}"#).unwrap();
        assert!(matches!(event, TiriEvent::WindowClosed { id: 42 }));
    }
}
