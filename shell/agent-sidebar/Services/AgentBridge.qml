import QtQuick
import Quickshell
import Quickshell.Io

Item {
  id: root

  property string ticShellRoot: ""
  property string workspaceKey: "workspace:default"
  property string workspaceTitle: "Workspace"
  property var events: []
  property string status: "starting"
  property var commands: []

  signal workspaceMessage(string title)

  function appendEvent(kind, title, body) {
    const next = events.slice();
    next.push({
      id: kind + ":" + next.length + ":" + Date.now(),
      kind: kind,
      title: title,
      body: body,
      time: Qt.formatTime(new Date(), "HH:mm")
    });
    events = next;
  }

  function setEvents(nextEvents) {
    events = nextEvents.map((entry, index) => ({
      id: entry.id || ("entry:" + index),
      kind: entry.kind || "system",
      title: entry.title || "Codex",
      body: entry.body || "",
      time: entry.time || Qt.formatTime(new Date(), "HH:mm")
    }));
  }

  function handleLine(line) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      return;
    }

    try {
      const message = JSON.parse(trimmed);
      if (message.type === "status") {
        status = message.status || "unknown";
      } else if (message.type === "snapshot") {
        setEvents(message.events || []);
      } else if (message.type === "workspace") {
        commands = message.commands || [];
        workspaceMessage(message.title || workspaceTitle);
      } else if (message.type === "event") {
        appendEvent(message.kind || "system", message.title || "Codex", message.body || "");
      }
    } catch (error) {
      appendEvent("stderr", "codex-agent", trimmed);
    }
  }

  function ensureRunning() {
    if (!codexAgent.running) {
      codexAgent.running = true;
    }
  }

  function start() {
    ensureRunning();
  }

  function writeMessage(message) {
    ensureRunning();
    codexAgent.write(JSON.stringify(message) + "\n");
  }

  function sendPrompt(prompt) {
    const trimmed = prompt.trim();
    if (trimmed.length === 0) {
      return;
    }

    writeMessage({
      type: "prompt",
      text: trimmed,
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle
    });
  }

  function sendControl(type) {
    writeMessage({
      type: type,
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle
    });
  }

  function notifyWorkspace() {
    if (!codexAgent.running) {
      return;
    }

    codexAgent.write(JSON.stringify({
      type: "workspace",
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle
    }) + "\n");
  }

  Process {
    id: codexAgent

    command: ["bun", root.ticShellRoot + "/bin/tic-codex-agent"]
    workingDirectory: Quickshell.env("HOME") || "/home/jettc"
    stdinEnabled: true
    running: false
    environment: ({
      "HOME": Quickshell.env("HOME") || "/home/jettc",
      "PATH": "/run/current-system/sw/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.local/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.cargo/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.bun/bin:" + (Quickshell.env("PATH") || ""),
      "TIC_CODEX_WORKDIR_ROOT": (Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/tic-shell/codex-workspaces"
    })

    stdout: SplitParser {
      onRead: data => root.handleLine(data)
    }

    stderr: SplitParser {
      onRead: data => {
        const trimmed = data.trim();
        if (trimmed.length > 0) {
          root.appendEvent("stderr", "codex-agent", trimmed);
        }
      }
    }

    onStarted: root.status = "starting"
    onExited: (exitCode, exitStatus) => {
      root.status = "stopped";
      root.appendEvent("system", "Codex agent stopped", "exit " + exitCode);
    }
  }

  Component.onCompleted: root.start()
}
