use serde::Serialize;
use tokio::sync::mpsc;

use crate::acp::{AgentCommand, AgentEntry, ForkSessionSummary};

#[derive(Debug, Clone)]
pub enum UiEvent {
    Status {
        status: String,
    },
    Snapshot {
        events: Vec<AgentEntry>,
    },
    Workspace {
        key: String,
        title: String,
        commands: Vec<AgentCommand>,
    },
    Event {
        kind: String,
        title: String,
        body: String,
    },
    WindowDescription {
        window_id: u64,
        description: String,
    },
    WorkspaceName {
        workspace_id: u64,
        name: String,
    },
    ForkSessions {
        sessions: Vec<ForkSessionSummary>,
    },
    ForkComplete {
        status: String,
        title: String,
        body: String,
        fork_id: String,
    },
    DebugSnapshot {
        snapshot: serde_json::Value,
    },
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum WireUiEvent<'a> {
    #[serde(rename = "status")]
    Status { status: &'a str },
    #[serde(rename = "snapshot")]
    Snapshot { events: &'a [AgentEntry] },
    #[serde(rename = "workspace")]
    Workspace {
        key: &'a str,
        title: &'a str,
        commands: &'a [AgentCommand],
    },
    #[serde(rename = "event")]
    Event {
        kind: &'a str,
        title: &'a str,
        body: &'a str,
    },
    #[serde(rename = "windowDescription")]
    WindowDescription {
        window_id: u64,
        description: &'a str,
    },
    #[serde(rename = "workspaceName")]
    WorkspaceName { workspace_id: u64, name: &'a str },
    #[serde(rename = "forkSessions")]
    ForkSessions { sessions: &'a [ForkSessionSummary] },
    #[serde(rename = "forkComplete")]
    ForkComplete {
        status: &'a str,
        title: &'a str,
        body: &'a str,
        #[serde(rename = "forkId")]
        fork_id: &'a str,
    },
    #[serde(rename = "debugSnapshot")]
    DebugSnapshot { snapshot: &'a serde_json::Value },
}

pub fn channel() -> (
    mpsc::UnboundedSender<UiEvent>,
    mpsc::UnboundedReceiver<UiEvent>,
) {
    mpsc::unbounded_channel()
}

pub async fn write_ui_events(mut rx: mpsc::UnboundedReceiver<UiEvent>) {
    while let Some(event) = rx.recv().await {
        let wire = match &event {
            UiEvent::Status { status } => WireUiEvent::Status { status },
            UiEvent::Snapshot { events } => WireUiEvent::Snapshot { events },
            UiEvent::Workspace {
                key,
                title,
                commands,
            } => WireUiEvent::Workspace {
                key,
                title,
                commands,
            },
            UiEvent::Event { kind, title, body } => WireUiEvent::Event { kind, title, body },
            UiEvent::WindowDescription {
                window_id,
                description,
            } => WireUiEvent::WindowDescription {
                window_id: *window_id,
                description,
            },
            UiEvent::WorkspaceName { workspace_id, name } => WireUiEvent::WorkspaceName {
                workspace_id: *workspace_id,
                name,
            },
            UiEvent::ForkSessions { sessions } => WireUiEvent::ForkSessions { sessions },
            UiEvent::ForkComplete {
                status,
                title,
                body,
                fork_id,
            } => WireUiEvent::ForkComplete {
                status,
                title,
                body,
                fork_id,
            },
            UiEvent::DebugSnapshot { snapshot } => WireUiEvent::DebugSnapshot { snapshot },
        };
        if let Ok(line) = serde_json::to_string(&wire) {
            println!("{line}");
        }
    }
}
