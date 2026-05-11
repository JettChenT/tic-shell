import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Widgets

Item {
  id: root

  property string ticShellRoot: ""
  property string coreBinary: Quickshell.env("TIC_SIDEBAR_CORE_BIN") || (ticShellRoot + "/target/release/tic-sidebar-core")
  property var workspaceRows: []
  property var windowRows: []
  property int windowRevision: 0
  property var windowStructureRows: []
  property int windowStructureRevision: 0
  property string lastWorkspaceRowsJson: ""
  property string lastWindowStructureRowsJson: ""
  property int activeWorkspaceId: -1
  property string activeWorkspaceLabel: "Workspace"

  signal agentWorkspaceChanged

  function workspaceKey(workspaceId) {
    return "niri:workspace:" + workspaceId;
  }

  function windowKey(windowId) {
    return "niri:window:" + windowId;
  }

  function currentAgentWorkspaceKey() {
    return activeWorkspaceId === -1 ? "workspace:default" : workspaceKey(activeWorkspaceId);
  }

  function windowsForWorkspace(workspaceId) {
    return windowStructureRows.filter(win => win.workspaceId === workspaceId);
  }

  function windowById(windowId) {
    for (let i = 0; i < windowRows.length; i++) {
      if (windowRows[i].id === windowId) {
        return windowRows[i];
      }
    }
    return null;
  }

  function windowTitle(windowId, revision) {
    const win = windowById(windowId);
    return win ? win.title : "(untitled)";
  }

  function windowFocused(windowId, revision) {
    const win = windowById(windowId);
    return win ? win.focused : false;
  }

  function appInitial(appId) {
    const normalized = (appId || "?").replace(/^com\./, "").replace(/^org\./, "");
    const parts = normalized.split(/[.\-_ ]+/).filter(part => part.length > 0);
    const token = parts.length > 0 ? parts[parts.length - 1] : normalized;
    return token.substring(0, 1).toUpperCase();
  }

  function iconForAppId(appId) {
    if (!appId || appId.length === 0) {
      return "";
    }

    const entry = DesktopEntries.heuristicLookup(appId);
    if (entry && entry.icon && entry.icon.length > 0) {
      const entryIcon = Quickshell.iconPath(entry.icon, true);
      if (entryIcon && entryIcon.length > 0) {
        return entryIcon;
      }
    }

    const directIcon = Quickshell.iconPath(appId, true);
    if (directIcon && directIcon.length > 0) {
      return directIcon;
    }

    const normalized = appId.replace(/^com\./, "").replace(/^org\./, "");
    const parts = normalized.split(/[.\-_ ]+/).filter(part => part.length > 0);
    for (let i = parts.length - 1; i >= 0; i--) {
      const partIcon = Quickshell.iconPath(parts[i].toLowerCase(), true);
      if (partIcon && partIcon.length > 0) {
        return partIcon;
      }
    }

    return "";
  }

  function ensureRunning() {
    if (!sidebarCore.running) {
      sidebarCore.running = true;
    }
  }

  function writeCommand(command) {
    ensureRunning();
    sidebarCore.write(JSON.stringify(command) + "\n");
  }

  function focusBottomWorkspace() {
    writeCommand({ type: "focus_workspace", idx: bottomWorkspaceIndex() });
  }

  function focusWorkspace(workspace) {
    writeCommand({ type: "focus_workspace", idx: workspace.idx });
  }

  function focusWindow(windowRow) {
    writeCommand({ type: "focus_window", id: windowRow.id });
  }

  function recenterColumns() {
    writeCommand({ type: "recenter_columns" });
  }

  function bottomWorkspaceIndex() {
    let targetOutput = "";
    for (let i = 0; i < workspaceRows.length; i++) {
      if (workspaceRows[i].focused || workspaceRows[i].active) {
        targetOutput = workspaceRows[i].output;
        break;
      }
    }

    let maxIdx = 1;
    for (let i = 0; i < workspaceRows.length; i++) {
      if (targetOutput.length === 0 || workspaceRows[i].output === targetOutput) {
        maxIdx = Math.max(maxIdx, workspaceRows[i].idx);
      }
    }
    return maxIdx;
  }

  function handleLine(line) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      return;
    }

    try {
      applyMessage(JSON.parse(trimmed));
    } catch (error) {
      console.warn("invalid sidebar core message", error, trimmed);
    }
  }

  function applyMessage(message) {
    if (message.type === "snapshot") {
      applySnapshot(message);
    } else if (message.type === "window_changed") {
      applyWindowChanged(message.window);
    } else if (message.type === "window_closed") {
      applyWindowClosed(message.id);
    } else if (message.type === "window_focus_changed") {
      applyWindowFocusChanged(message.id);
    } else if (message.type === "error") {
      console.warn("sidebar core error", message.message || "");
    }
  }

  function applySnapshot(snapshot) {
    const rows = snapshot.workspaces || [];
    const windows = sortWindows(snapshot.windows || []);
    const nextActiveWorkspaceId = snapshot.activeWorkspaceId === undefined ? -1 : snapshot.activeWorkspaceId;

    if (nextActiveWorkspaceId !== activeWorkspaceId) {
      activeWorkspaceId = nextActiveWorkspaceId;
      activeWorkspaceLabel = snapshot.activeWorkspaceLabel || "Workspace";
      agentWorkspaceChanged();
    }

    const nextRowsJson = JSON.stringify(workspaceRowsSignature(rows));
    if (nextRowsJson !== lastWorkspaceRowsJson) {
      lastWorkspaceRowsJson = nextRowsJson;
      workspaceRows = rows;
    }

    setWindows(windows);
  }

  function applyWindowChanged(windowRow) {
    if (!windowRow) {
      return;
    }

    const next = windowRows.slice();
    let replaced = false;
    for (let i = 0; i < next.length; i++) {
      if (next[i].id === windowRow.id) {
        next[i] = windowRow;
        replaced = true;
        break;
      }
    }
    if (!replaced) {
      next.push(windowRow);
    }
    setWindows(sortWindows(next));
  }

  function applyWindowClosed(windowId) {
    setWindows(windowRows.filter(win => win.id !== windowId));
  }

  function applyWindowFocusChanged(windowId) {
    const next = windowRows.map(win => Object.assign({}, win, { focused: win.id === windowId }));
    setWindows(next);
  }

  function setWindows(windows) {
    const nextWindowStructureRows = windows.map(win => ({
      id: win.id,
      key: win.key || windowKey(win.id),
      appId: win.appId || "",
      workspaceId: win.workspaceId === undefined ? -1 : win.workspaceId,
      floating: !!win.floating,
      positionX: win.positionX || 0,
      positionY: win.positionY || 0
    }));
    const nextWindowStructureRowsJson = JSON.stringify(nextWindowStructureRows);
    if (nextWindowStructureRowsJson !== lastWindowStructureRowsJson) {
      lastWindowStructureRowsJson = nextWindowStructureRowsJson;
      windowStructureRows = nextWindowStructureRows;
      windowStructureRevision++;
    }

    windowRows = windows;
    windowRevision++;
  }

  function sortWindows(windows) {
    return windows.slice().sort((a, b) => {
      const aw = a.workspaceId === undefined ? -1 : a.workspaceId;
      const bw = b.workspaceId === undefined ? -1 : b.workspaceId;
      if (aw !== bw) {
        return aw - bw;
      }
      if ((a.positionX || 0) !== (b.positionX || 0)) {
        return (a.positionX || 0) - (b.positionX || 0);
      }
      if ((a.positionY || 0) !== (b.positionY || 0)) {
        return (a.positionY || 0) - (b.positionY || 0);
      }
      return a.id - b.id;
    });
  }

  function workspaceRowsSignature(rows) {
    return rows.map(row => ({
      id: row.id,
      idx: row.idx,
      name: row.name,
      label: row.label,
      output: row.output,
      focused: row.focused,
      active: row.active,
      urgent: row.urgent,
      occupied: row.occupied,
      activeWindowId: row.activeWindowId
    }));
  }

  Component.onCompleted: ensureRunning()

  Process {
    id: sidebarCore

    command: [root.coreBinary]
    stdinEnabled: true
    running: false

    stdout: SplitParser {
      onRead: data => root.handleLine(data)
    }

    stderr: SplitParser {
      onRead: data => {
        const trimmed = data.trim();
        if (trimmed.length > 0) {
          console.warn("sidebar core", trimmed);
        }
      }
    }

    onExited: (exitCode, exitStatus) => {
      console.warn("sidebar core stopped", exitCode, exitStatus);
      restartTimer.restart();
    }
  }

  Timer {
    id: restartTimer

    interval: 500
    repeat: false
    onTriggered: root.ensureRunning()
  }
}
