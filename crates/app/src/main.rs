use std::{io::Write, os::unix::net::UnixStream, path::PathBuf};

use anyhow::{Context as _, Result};
use gpui_platform::application;
use shell_sidebar::{IPC_SOCKET_BASENAME, Sidebar, SidebarCommand};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    if args.action != Action::Start && signal_existing(args.action).is_ok() {
        return Ok(());
    }

    let repo_root = repo_root();
    let workdir = std::env::var_os("TIC_CODEX_WORKDIR")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.clone());

    let annotations = persistence::AnnotationStore::load_default()
        .context("failed to load workspace annotations")?;
    let (agent_bridge, agent_updates) = match agent::AgentBridge::spawn(repo_root, workdir).await {
        Ok((bridge, updates)) => (Some(bridge), Some(updates)),
        Err(err) => {
            tracing::warn!("starting without agent bridge: {err:#}");
            (None, None)
        }
    };

    let app = application();
    app.run(move |cx| {
        let handle = shell_sidebar::open(annotations, agent_bridge, agent_updates, cx)
            .expect("failed to open tic-shell sidebar");
        start_ipc(handle, cx);
    });

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Start,
    Stop,
    Toggle,
    Show,
    Hide,
    ToggleAgent,
    ShowAgent,
    HideAgent,
}

impl Action {
    fn as_command(self) -> Option<SidebarCommand> {
        match self {
            Self::Toggle => Some(SidebarCommand::Toggle),
            Self::Show => Some(SidebarCommand::Show),
            Self::Hide => Some(SidebarCommand::Hide),
            Self::ToggleAgent => Some(SidebarCommand::ToggleAgent),
            Self::ShowAgent => Some(SidebarCommand::ShowAgent),
            Self::HideAgent => Some(SidebarCommand::HideAgent),
            Self::Start | Self::Stop => None,
        }
    }

    fn encode(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Toggle => "toggle",
            Self::Show => "show",
            Self::Hide => "hide",
            Self::ToggleAgent => "toggle-agent",
            Self::ShowAgent => "show-agent",
            Self::HideAgent => "hide-agent",
        }
    }

    fn decode(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "start" => Self::Start,
            "stop" => Self::Stop,
            "toggle" => Self::Toggle,
            "show" | "reveal" => Self::Show,
            "hide" => Self::Hide,
            "toggle-agent" | "toggleAgent" => Self::ToggleAgent,
            "show-agent" | "reveal-agent" | "revealAgent" => Self::ShowAgent,
            "hide-agent" | "hideAgent" => Self::HideAgent,
            _ => return None,
        })
    }
}

struct Args {
    action: Action,
}

impl Args {
    fn parse() -> Self {
        let action = std::env::args()
            .nth(1)
            .as_deref()
            .and_then(Action::decode)
            .unwrap_or(Action::Start);
        Self { action }
    }
}

fn start_ipc(handle: gpui::WindowHandle<Sidebar>, cx: &mut gpui::App) {
    let path = socket_path();
    if path.exists()
        && let Err(err) = std::fs::remove_file(&path)
    {
        tracing::warn!(
            "failed to remove stale IPC socket {}: {err}",
            path.display()
        );
    }

    let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            tracing::warn!("failed to bind IPC socket {}: {err}", path.display());
            return;
        }
    };

    cx.spawn(async move |cx| {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let mut bytes = Vec::new();
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut bytes).await;
            let message = String::from_utf8_lossy(&bytes);
            let Some(action) = Action::decode(&message) else {
                continue;
            };
            if action == Action::Stop {
                cx.update(|cx| cx.quit());
                break;
            }
            let Some(command) = action.as_command() else {
                continue;
            };
            let handle = handle.clone();
            let _ = cx.update(move |cx| {
                let _ = handle.update(cx, |sidebar, _window, cx| {
                    sidebar.command(command, cx);
                });
            });
        }
    })
    .detach();
}

fn signal_existing(action: Action) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path())?;
    stream.write_all(action.encode().as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write).ok();
    Ok(())
}

fn socket_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join(IPC_SOCKET_BASENAME)
    } else {
        PathBuf::from("/tmp").join(IPC_SOCKET_BASENAME)
    }
}

fn repo_root() -> PathBuf {
    if let Some(root) = std::env::var_os("TIC_SHELL_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
