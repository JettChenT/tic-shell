import QtQuick
import Quickshell
import Quickshell.Io
import qs.Modules.TicWorkspace

Item {
  id: root

  property string ticShellRoot: ""
  property string workspaceKey: "workspace:default"
  property string workspaceTitle: "Workspace"
  property var events: []
  property string status: "starting"
  property var commands: []
  property var forkSessions: []
  readonly property var windowDescriptions: TicWorkspaceState.windowDescriptions
  property var debugSnapshot: ({})

  signal workspaceMessage(string title)
  signal workspaceNameSet(int workspaceId, string name)
  signal forkComplete(string status, string title, string body)

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
      title: entry.title || "Agent",
      body: entry.body || "",
      metadata: entry.metadata || {},
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
        appendEvent(message.kind || "system", message.title || "Agent", message.body || "");
      } else if (message.type === "forkSessions") {
        forkSessions = message.sessions || [];
      } else if (message.type === "forkComplete") {
        forkComplete(message.status || "done", message.title || "Fork cursor", message.body || "");
      } else if (message.type === "windowDescription") {
        const windowId = String(message.window_id || "");
        if (windowId.length > 0) {
          TicWorkspaceState.setWindowDescription(windowId, message.description || "");
        }
      } else if (message.type === "workspaceName") {
        const workspaceId = Number(message.workspace_id || 0);
        if (workspaceId > 0) {
          workspaceNameSet(workspaceId, message.name || "");
        }
      } else if (message.type === "debugSnapshot") {
        debugSnapshot = message.snapshot || {};
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

  function prepareForkCursor(forkId, activeWindow) {
    if (!forkId || forkId.length === 0) {
      return;
    }

    writeMessage({
      type: "prepare-fork-cursor",
      forkId: forkId,
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle,
      activeWindow: activeWindow || null
    });
  }

  function sendForkCursorPrompt(forkId, prompt, activeWindow) {
    const trimmed = prompt.trim();
    if (trimmed.length === 0) {
      return;
    }

    writeMessage({
      type: "fork-cursor",
      forkId: forkId || "",
      text: trimmed,
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle,
      activeWindow: activeWindow || null
    });
  }

  function selectFork(id) {
    if (!id || id.length === 0) {
      return;
    }

    writeMessage({
      type: "select-fork",
      id: id
    });
  }

  function dismissFork(id) {
    if (!id || id.length === 0) {
      return;
    }

    writeMessage({
      type: "dismiss-fork",
      id: id
    });
  }

  function sendControl(type) {
    writeMessage({
      type: type,
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle
    });
  }

  function deactivate() {
    if (!codexAgent.running) {
      return;
    }

    codexAgent.write(JSON.stringify({
      type: "deactivate",
      workspaceKey: workspaceKey,
      workspaceTitle: workspaceTitle
    }) + "\n");
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

    command: ["cargo", "run", "--quiet", "--bin", "tic-daemon"]
    workingDirectory: root.ticShellRoot
    stdinEnabled: true
    running: false
    environment: ({
      "HOME": Quickshell.env("HOME") || "/home/jettc",
      "PATH": "/run/current-system/sw/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.local/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.cargo/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.bun/bin:" + (Quickshell.env("PATH") || ""),
      "TIC_SHELL_ROOT": root.ticShellRoot,
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
      root.appendEvent("system", "Agent stopped", "exit " + exitCode);
    }
  }

  Component.onCompleted: root.start()
}
