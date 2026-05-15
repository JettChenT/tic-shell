mod acp;
mod collab;
mod config;
mod heart;
mod mcp;
mod niri;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{acp::AcpHandle, config::Config, heart::HeartSupervisor};

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "mcp") {
        let event_log = arg_value(&args, "--event-log")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("TIC_DAEMON_EVENT_LOG").map(PathBuf::from))
            .unwrap_or_else(|| config::home_dir().join(".tic").join("events.jsonl"));
        return mcp::run_mcp(event_log).await;
    }
    if args.get(1).is_some_and(|arg| arg == "collab") {
        return collab::run(&args[2..]).await;
    }

    let config = Config::load_or_create()?;
    let repo_root = repo_root()?;
    let (ui_tx, ui_rx) = ui::channel();
    tokio::spawn(ui::write_ui_events(ui_rx));

    let (acp, mut child) = AcpHandle::spawn(config.clone(), repo_root, ui_tx.clone()).await?;
    HeartSupervisor::spawn(config, acp.clone(), ui_tx.clone());
    let stdin_task = tokio::spawn(read_stdin(acp));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = stdin_task => {}
        status = child.wait() => {
            let _ = status;
        }
    }

    Ok(())
}

async fn read_stdin(acp: AcpHandle) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(value) => acp.handle_ui_input(value).await,
            Err(err) => eprintln!("invalid daemon input: {err}"),
        }
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}

fn repo_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("TIC_SHELL_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(PathBuf::from)
        .context("resolve repo root from manifest")
}
