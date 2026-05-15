use std::{
    os::fd::AsRawFd,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use ashpd::desktop::{
    PersistMode,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWindowSpec {
    pub peer_id: String,
    pub remote_window_id: u64,
    pub remote_workspace_id: Option<u64>,
    pub title: String,
    pub app_id: Option<String>,
    pub width: u32,
    pub height: u32,
    pub stream_id: String,
}

pub async fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("share-window") => share_window(args),
        Some("stop-window") => stop_window(args),
        Some("open-remote-window") => open_remote_window(args),
        Some("focus-remote-window") => focus_remote_window(args),
        Some("close-remote-window") => close_remote_window(args),
        Some("remote-windows") => niri_json(["--json", "remote-windows"]),
        Some("shared-window-streams") => niri_json(["--json", "shared-window-streams"]),
        Some("publish-window") => publish_window(args).await,
        Some("view-window") => view_window(args),
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => bail!("unknown collab command {command:?}; run `tic-daemon collab help`"),
    }
}

async fn publish_window(args: &[String]) -> Result<()> {
    let id = required_arg(args, "--id")?;
    let stream_id = optional_arg(args, "--stream-id")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("local-window-{id}"));
    share_window(&[
        "share-window".to_string(),
        "--id".to_string(),
        id.to_string(),
        "--stream-id".to_string(),
        stream_id.clone(),
    ])?;

    let proxy = Screencast::new()
        .await
        .context("connect to ScreenCast portal")?;
    let session = proxy
        .create_session(Default::default())
        .await
        .context("create ScreenCast session")?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Metadata)
                .set_sources(SourceType::Window | SourceType::Window)
                .set_multiple(false)
                .set_persist_mode(PersistMode::DoNot),
        )
        .await
        .context("select ScreenCast source")?;

    let response = proxy
        .start(&session, None, Default::default())
        .await
        .context("start ScreenCast session")?
        .response()
        .context("read ScreenCast response")?;
    let stream = response
        .streams()
        .first()
        .ok_or_else(|| anyhow!("ScreenCast portal returned no streams"))?;
    let node_id = stream.pipe_wire_node_id();
    let fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .context("open PipeWire remote for ScreenCast session")?;
    let fd_raw = fd.as_raw_fd();

    eprintln!(
        "{}",
        serde_json::json!({
            "published": true,
            "window_id": id,
            "stream_id": stream_id,
            "pipewire_node_id": node_id,
            "size": stream.size(),
            "position": stream.position(),
            "signalling": "webrtcsink default signalling server"
        })
    );

    let mut command = Command::new("gst-launch-1.0");
    command.args([
        "-v",
        "pipewiresrc",
        &format!("fd={fd_raw}"),
        &format!("path={node_id}"),
        "do-timestamp=true",
        "!",
        "queue",
        "leaky=downstream",
        "max-size-buffers=2",
        "!",
        "videoconvert",
        "!",
        "webrtcsink",
        "name=ws",
        "run-signalling-server=true",
        "run-web-server=true",
    ]);
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd_raw, libc::F_GETFD);
            if flags < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::fcntl(fd_raw, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let status = command.status().context("run GStreamer WebRTC publisher")?;
    if !status.success() {
        bail!("GStreamer WebRTC publisher exited with {status}");
    }
    Ok(())
}

fn view_window(args: &[String]) -> Result<()> {
    let peer_id = required_arg(args, "--peer-id")?;
    let remote_window_id = required_arg(args, "--remote-window-id")?;
    let title = required_arg(args, "--title")?;
    let stream_id = required_arg(args, "--stream-id")?;
    let producer_peer_id = required_arg(args, "--producer-peer-id")?;
    let signaller_uri = optional_arg(args, "--signaller-uri").unwrap_or("ws://127.0.0.1:8443");

    open_remote_window(args)?;

    eprintln!(
        "{}",
        serde_json::json!({
            "viewing": true,
            "peer_id": peer_id,
            "remote_window_id": remote_window_id,
            "title": title,
            "stream_id": stream_id,
            "producer_peer_id": producer_peer_id,
            "signaller_uri": signaller_uri
        })
    );

    let status = Command::new("gst-launch-1.0")
        .args([
            "-v",
            "webrtcsrc",
            &format!("signaller::uri={signaller_uri}"),
            &format!("signaller::producer-peer-id={producer_peer_id}"),
            "!",
            "queue",
            "!",
            "videoconvert",
            "!",
            "waylandsink",
            "sync=false",
        ])
        .status()
        .context("run GStreamer WebRTC viewer")?;
    if !status.success() {
        bail!("GStreamer WebRTC viewer exited with {status}");
    }
    Ok(())
}

fn share_window(args: &[String]) -> Result<()> {
    let id = required_arg(args, "--id")?;
    let mut command = vec!["msg", "action", "share-window-stream", "--id", id];
    if let Some(stream_id) = optional_arg(args, "--stream-id") {
        command.extend(["--stream-id", stream_id]);
    }
    niri(command)
}

fn stop_window(args: &[String]) -> Result<()> {
    let id = required_arg(args, "--id")?;
    niri(["msg", "action", "stop-window-stream", "--id", id])
}

fn open_remote_window(args: &[String]) -> Result<()> {
    let spec = if let Some(json) = optional_arg(args, "--json") {
        serde_json::from_str::<RemoteWindowSpec>(json).context("parse remote window JSON")?
    } else {
        RemoteWindowSpec {
            peer_id: required_arg(args, "--peer-id")?.to_string(),
            remote_window_id: required_arg(args, "--remote-window-id")?.parse()?,
            remote_workspace_id: optional_arg(args, "--remote-workspace-id")
                .map(str::parse)
                .transpose()?,
            title: required_arg(args, "--title")?.to_string(),
            app_id: optional_arg(args, "--app-id").map(str::to_string),
            width: optional_arg(args, "--width")
                .unwrap_or("1280")
                .parse()
                .context("parse --width")?,
            height: optional_arg(args, "--height")
                .unwrap_or("720")
                .parse()
                .context("parse --height")?,
            stream_id: required_arg(args, "--stream-id")?.to_string(),
        }
    };

    let mut command = vec![
        "msg".to_string(),
        "action".to_string(),
        "open-remote-window".to_string(),
        "--peer-id".to_string(),
        spec.peer_id,
        "--remote-window-id".to_string(),
        spec.remote_window_id.to_string(),
        "--title".to_string(),
        spec.title,
        "--width".to_string(),
        spec.width.to_string(),
        "--height".to_string(),
        spec.height.to_string(),
        "--stream-id".to_string(),
        spec.stream_id,
    ];
    if let Some(workspace_id) = spec.remote_workspace_id {
        command.extend([
            "--remote-workspace-id".to_string(),
            workspace_id.to_string(),
        ]);
    }
    if let Some(app_id) = spec.app_id {
        command.extend(["--app-id".to_string(), app_id]);
    }
    niri(command)
}

fn focus_remote_window(args: &[String]) -> Result<()> {
    let id = required_arg(args, "--id")?;
    niri(["msg", "action", "focus-remote-window", "--id", id])
}

fn close_remote_window(args: &[String]) -> Result<()> {
    let id = required_arg(args, "--id")?;
    niri(["msg", "action", "close-remote-window", "--id", id])
}

fn niri<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = niri_command(args).output().context("run niri msg")?;
    if !output.status.success() {
        return Err(anyhow!(
            "niri command failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn niri_json<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    niri(args)
}

fn niri_command<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new(std::env::var("TIC_NIRI_BIN").unwrap_or_else(|_| "niri".into()));
    command.stdin(Stdio::null());
    for arg in args {
        command.arg(arg.as_ref());
    }
    command
}

fn required_arg<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    optional_arg(args, name).ok_or_else(|| anyhow!("missing required argument {name}"))
}

fn optional_arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
}

fn print_help() {
    println!(
        "tic-daemon collab commands:\n\
         share-window --id <window-id> [--stream-id <stream-id>]\n\
         stop-window --id <window-id>\n\
         open-remote-window --peer-id <peer> --remote-window-id <id> --title <title> --stream-id <stream> [--remote-workspace-id <id>] [--app-id <app>] [--width <px>] [--height <px>]\n\
         focus-remote-window --id <local-remote-window-id>\n\
         close-remote-window --id <local-remote-window-id>\n\
         remote-windows\n\
         shared-window-streams\n\
         publish-window --id <window-id> [--stream-id <stream-id>]\n\
         view-window --peer-id <peer> --remote-window-id <id> --title <title> --stream-id <stream> --producer-peer-id <webrtcsink-peer-id> [--signaller-uri <ws-uri>]"
    );
}
