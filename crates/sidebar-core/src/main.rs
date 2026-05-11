use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use services::{NiriClient, NiriUpdate, NiriWindow, NiriWorkspace, WorkspaceFocus, WorkspaceSnapshot};
use tokio::{
    io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match NiriClient::stream_updates(event_tx.clone()).await {
                Ok(()) => {}
                Err(err) => {
                    tracing::warn!("niri event stream unavailable: {err:#}");
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
            if event_tx.is_closed() {
                break;
            }
        }
    });

    let (command_tx, mut command_rx) = mpsc::unbounded_channel();
    tokio::spawn(read_commands(command_tx));

    let stdout = tokio::io::stdout();
    let mut stdout = tokio::io::BufWriter::new(stdout);

    if let Ok(snapshot) = NiriClient::snapshot() {
        write_event(&mut stdout, &CoreEvent::snapshot(snapshot)).await?;
    }

    loop {
        tokio::select! {
            Some(update) = event_rx.recv() => {
                write_event(&mut stdout, &CoreEvent::from(update)).await?;
            }
            Some(command) = command_rx.recv() => {
                if let Err(err) = handle_command(command).await {
                    write_event(
                        &mut stdout,
                        &CoreEvent::Error {
                            message: format!("{err:#}"),
                        },
                    )
                    .await?;
                }
            }
            else => break,
        }
    }

    Ok(())
}

async fn read_commands(sender: mpsc::UnboundedSender<CoreCommand>) {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<CoreCommand>(trimmed) {
            Ok(command) => {
                if sender.send(command).is_err() {
                    break;
                }
            }
            Err(err) => tracing::warn!("invalid sidebar command {trimmed:?}: {err}"),
        }
    }
}

async fn handle_command(command: CoreCommand) -> Result<()> {
    match command {
        CoreCommand::FocusWorkspace { idx } => {
            NiriClient::focus_workspace(idx).context("failed to focus workspace")?;
        }
        CoreCommand::FocusWindow { id } => {
            NiriClient::focus_window(id).context("failed to focus window")?;
        }
        CoreCommand::RecenterColumns => {
            NiriClient::recenter_columns().context("failed to recenter columns")?;
        }
        CoreCommand::Refresh => {
            // The event stream sends initial state on connect. A no-op refresh command is kept
            // for QML callers that want to ensure the process is running.
        }
    }
    Ok(())
}

async fn write_event<W>(writer: &mut W, event: &CoreEvent) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CoreCommand {
    FocusWorkspace { idx: i64 },
    FocusWindow { id: u64 },
    RecenterColumns,
    Refresh,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
enum CoreEvent {
    Snapshot {
        workspaces: Vec<SidebarWorkspace>,
        windows: Vec<SidebarWindow>,
        active_workspace_id: i64,
        active_workspace_label: String,
    },
    WindowChanged { window: SidebarWindow },
    WindowClosed { id: u64 },
    WindowFocusChanged { id: Option<u64> },
    Error { message: String },
}

impl From<NiriUpdate> for CoreEvent {
    fn from(update: NiriUpdate) -> Self {
        match update {
            NiriUpdate::Snapshot(snapshot) => Self::snapshot(snapshot),
            NiriUpdate::WindowChanged(window) => Self::WindowChanged {
                window: window.into(),
            },
            NiriUpdate::WindowClosed(id) => Self::WindowClosed { id },
            NiriUpdate::WindowFocusChanged(id) => Self::WindowFocusChanged { id },
        }
    }
}

impl CoreEvent {
    fn snapshot(snapshot: WorkspaceSnapshot) -> Self {
        Self::Snapshot {
            workspaces: snapshot.workspaces.into_iter().map(Into::into).collect(),
            windows: snapshot.windows.into_iter().map(Into::into).collect(),
            active_workspace_id: snapshot.active_workspace_id.map(|id| id as i64).unwrap_or(-1),
            active_workspace_label: snapshot.active_workspace_label,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidebarWorkspace {
    id: u64,
    key: String,
    idx: i64,
    name: String,
    label: String,
    output: String,
    focused: bool,
    active: bool,
    urgent: bool,
    occupied: bool,
    active_window_id: u64,
}

impl From<NiriWorkspace> for SidebarWorkspace {
    fn from(workspace: NiriWorkspace) -> Self {
        let focused = matches!(workspace.focus, WorkspaceFocus::Focused);
        let active = matches!(workspace.focus, WorkspaceFocus::Active);
        Self {
            id: workspace.id,
            key: workspace.key,
            idx: workspace.idx,
            name: workspace.name,
            label: workspace.label,
            output: workspace.output,
            focused,
            active,
            urgent: workspace.urgent,
            occupied: workspace.active_window_id.is_some(),
            active_window_id: workspace.active_window_id.unwrap_or(0),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidebarWindow {
    id: u64,
    key: String,
    title: String,
    app_id: String,
    workspace_id: i64,
    focused: bool,
    floating: bool,
    position_x: i64,
    position_y: i64,
}

impl From<NiriWindow> for SidebarWindow {
    fn from(window: NiriWindow) -> Self {
        Self {
            id: window.id,
            key: window.key,
            title: window.title,
            app_id: window.app_id,
            workspace_id: window.workspace_id.map(|id| id as i64).unwrap_or(-1),
            focused: window.focused,
            floating: window.floating,
            position_x: window.position_x,
            position_y: window.position_y,
        }
    }
}
