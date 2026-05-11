use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{Mutex, mpsc},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentEvent {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentUpdate {
    Status {
        workspace_key: Option<String>,
        status: String,
    },
    Snapshot {
        workspace_key: Option<String>,
        events: Vec<AgentEvent>,
    },
    Workspace {
        key: Option<String>,
        title: String,
        commands: Vec<AgentCommand>,
    },
    Event(AgentEvent),
    Stderr(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum UiRequest<'a> {
    #[serde(rename = "workspace")]
    Workspace {
        #[serde(rename = "workspaceKey")]
        workspace_key: &'a str,
        #[serde(rename = "workspaceTitle")]
        workspace_title: &'a str,
    },
    #[serde(rename = "prompt")]
    Prompt {
        text: &'a str,
        #[serde(rename = "workspaceKey")]
        workspace_key: &'a str,
        #[serde(rename = "workspaceTitle")]
        workspace_title: &'a str,
    },
    #[serde(rename = "clear")]
    Clear {
        #[serde(rename = "workspaceKey")]
        workspace_key: &'a str,
        #[serde(rename = "workspaceTitle")]
        workspace_title: &'a str,
    },
    #[serde(rename = "new")]
    New {
        #[serde(rename = "workspaceKey")]
        workspace_key: &'a str,
        #[serde(rename = "workspaceTitle")]
        workspace_title: &'a str,
    },
    #[serde(rename = "cancel")]
    Cancel {
        #[serde(rename = "workspaceKey")]
        workspace_key: &'a str,
        #[serde(rename = "workspaceTitle")]
        workspace_title: &'a str,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum BridgeLine {
    #[serde(rename = "status")]
    Status {
        #[serde(rename = "workspaceKey")]
        workspace_key: Option<String>,
        status: String,
    },
    #[serde(rename = "snapshot")]
    Snapshot {
        #[serde(rename = "workspaceKey")]
        workspace_key: Option<String>,
        #[serde(default)]
        events: Vec<AgentEvent>,
    },
    #[serde(rename = "workspace")]
    Workspace {
        key: Option<String>,
        #[serde(default)]
        title: String,
        #[serde(default)]
        commands: Vec<AgentCommand>,
    },
    #[serde(rename = "event")]
    Event {
        #[serde(default)]
        id: String,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        time: String,
    },
}

#[derive(Clone)]
pub struct AgentBridge {
    stdin: Arc<Mutex<ChildStdin>>,
}

impl AgentBridge {
    pub async fn spawn(
        repo_root: PathBuf,
        workdir: PathBuf,
    ) -> Result<(Self, mpsc::UnboundedReceiver<AgentUpdate>)> {
        let bridge = repo_root.join("bin/tic-codex-agent");
        let mut child = Command::new("bun")
            .arg(&bridge)
            .current_dir(&workdir)
            .env("TIC_CODEX_WORKDIR", &workdir)
            .env("TIC_SHELL_ROOT", &repo_root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn {}", bridge.display()))?;

        let stdin = child
            .stdin
            .take()
            .context("agent bridge stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("agent bridge stdout unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("agent bridge stderr unavailable")?;
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(read_stdout(stdout, tx.clone()));
        tokio::spawn(read_stderr(stderr, tx));

        Ok((
            Self {
                stdin: Arc::new(Mutex::new(stdin)),
            },
            rx,
        ))
    }

    pub async fn notify_workspace(&self, workspace_key: &str, workspace_title: &str) -> Result<()> {
        self.write(UiRequest::Workspace {
            workspace_key,
            workspace_title,
        })
        .await
    }

    pub async fn prompt(
        &self,
        workspace_key: &str,
        workspace_title: &str,
        text: &str,
    ) -> Result<()> {
        self.write(UiRequest::Prompt {
            text,
            workspace_key,
            workspace_title,
        })
        .await
    }

    pub async fn control(
        &self,
        workspace_key: &str,
        workspace_title: &str,
        control: AgentControl,
    ) -> Result<()> {
        match control {
            AgentControl::Clear => {
                self.write(UiRequest::Clear {
                    workspace_key,
                    workspace_title,
                })
                .await
            }
            AgentControl::New => {
                self.write(UiRequest::New {
                    workspace_key,
                    workspace_title,
                })
                .await
            }
            AgentControl::Cancel => {
                self.write(UiRequest::Cancel {
                    workspace_key,
                    workspace_title,
                })
                .await
            }
        }
    }

    async fn write(&self, request: UiRequest<'_>) -> Result<()> {
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&line).await?;
        stdin.flush().await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentControl {
    Clear,
    New,
    Cancel,
}

async fn read_stdout(stdout: tokio::process::ChildStdout, tx: mpsc::UnboundedSender<AgentUpdate>) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let update = parse_bridge_line(&line).unwrap_or_else(|| AgentUpdate::Stderr(line));
        if tx.send(update).is_err() {
            break;
        }
    }
}

async fn read_stderr(stderr: tokio::process::ChildStderr, tx: mpsc::UnboundedSender<AgentUpdate>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if tx.send(AgentUpdate::Stderr(line)).is_err() {
            break;
        }
    }
}

pub fn parse_bridge_line(line: &str) -> Option<AgentUpdate> {
    let parsed: BridgeLine = serde_json::from_str(line.trim()).ok()?;
    Some(match parsed {
        BridgeLine::Status {
            workspace_key,
            status,
        } => AgentUpdate::Status {
            workspace_key,
            status,
        },
        BridgeLine::Snapshot {
            workspace_key,
            events,
        } => AgentUpdate::Snapshot {
            workspace_key,
            events,
        },
        BridgeLine::Workspace {
            key,
            title,
            commands,
        } => AgentUpdate::Workspace {
            key,
            title,
            commands,
        },
        BridgeLine::Event {
            id,
            kind,
            title,
            body,
            time,
        } => AgentUpdate::Event(AgentEvent {
            id,
            kind,
            title,
            body,
            time,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bridge_snapshot_lines() {
        let update = parse_bridge_line(
            r#"{"type":"snapshot","workspaceKey":"niri:workspace:1","events":[{"id":"a","kind":"assistant","title":"Codex","body":"hi","time":"10:00"}]}"#,
        )
        .unwrap();

        assert_eq!(
            update,
            AgentUpdate::Snapshot {
                workspace_key: Some("niri:workspace:1".to_string()),
                events: vec![AgentEvent {
                    id: "a".to_string(),
                    kind: "assistant".to_string(),
                    title: "Codex".to_string(),
                    body: "hi".to_string(),
                    time: "10:00".to_string(),
                }],
            }
        );
    }
}
