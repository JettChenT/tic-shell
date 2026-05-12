import QtQuick
import Quickshell
import Quickshell.Niri
import Quickshell.Widgets

Item {
  id: root

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

  function displayWorkspaceName(workspace) {
    if (workspace.name && workspace.name.length > 0) {
      return workspace.name;
    }
    return String(workspace.idx);
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

  function refreshState() {
    const workspaces = Niri.workspaces.values.slice().sort((a, b) => {
      if (a.output !== b.output) {
        return a.output < b.output ? -1 : 1;
      }
      return a.idx - b.idx;
    });
    const windows = Niri.windows.values.map(win => ({
      id: win.id,
      key: windowKey(win.id),
      title: win.title || "(untitled)",
      appId: win.appId || "",
      workspaceId: win.workspaceId || -1,
      focused: win.focused,
      floating: win.isFloating,
      positionX: win.positionX,
      positionY: win.positionY
    })).sort((a, b) => {
      if (a.workspaceId !== b.workspaceId) {
        return a.workspaceId - b.workspaceId;
      }
      if (a.positionX !== b.positionX) {
        return a.positionX - b.positionX;
      }
      if (a.positionY !== b.positionY) {
        return a.positionY - b.positionY;
      }
      return a.id - b.id;
    });
    let nextActiveWorkspaceId = -1;

    const rows = workspaces.map(ws => ({
      id: ws.id,
      key: workspaceKey(ws.id),
      idx: ws.idx,
      name: ws.name || "",
      label: displayWorkspaceName(ws),
      output: ws.output || "",
      focused: ws.focused,
      active: ws.active,
      urgent: ws.urgent,
      occupied: ws.occupied,
      activeWindowId: ws.activeWindowId || 0
    }));

    for (let i = 0; i < rows.length; i++) {
      if (rows[i].focused || rows[i].active) {
        nextActiveWorkspaceId = rows[i].id;
        break;
      }
    }

    if (nextActiveWorkspaceId !== -1 && nextActiveWorkspaceId !== activeWorkspaceId) {
      activeWorkspaceId = nextActiveWorkspaceId;
      const activeRow = rows.find(row => row.id === nextActiveWorkspaceId);
      activeWorkspaceLabel = activeRow ? activeRow.label : "Workspace";
      agentWorkspaceChanged();
    }

    const nextRowsJson = JSON.stringify(workspaceRowsSignature(rows));
    if (nextRowsJson !== lastWorkspaceRowsJson) {
      lastWorkspaceRowsJson = nextRowsJson;
      workspaceRows = rows;
    }

    const nextWindowStructureRows = windows.map(win => ({
      id: win.id,
      key: win.key,
      appId: win.appId,
      workspaceId: win.workspaceId,
      floating: win.floating,
      positionX: win.positionX,
      positionY: win.positionY
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

  function focusBottomWorkspace() {
    Niri.dispatch(["focus-workspace", String(bottomWorkspaceIndex())]);
  }

  function focusWorkspace(workspace) {
    Niri.dispatch(["focus-workspace", String(workspace.idx)]);
  }

  function focusWindow(windowRow) {
    Niri.dispatch(["focus-window", "--id", String(windowRow.id)]);
  }

  function showWindowPreview(windowRow, x, y, rowHeight) {
    const previewWidth = 360;
    const previewHeight = 220;
    const previewX = Math.max(0, Math.round(x));
    const previewY = Math.max(0, Math.round(y - previewHeight / 2));

    Niri.dispatch([
      "show-window-preview",
      "--id", String(windowRow.id),
      "--x", String(previewX),
      "--y", String(previewY),
      "--width", String(previewWidth),
      "--height", String(previewHeight)
    ]);
  }

  function hideWindowPreview() {
    Niri.dispatch(["hide-window-preview"]);
  }

  function recenterColumns() {
    let focusedWindowId = -1;
    for (let i = 0; i < windowRows.length; i++) {
      if (windowRows[i].focused) {
        focusedWindowId = windowRows[i].id;
        break;
      }
    }

    Niri.dispatch(["focus-column-first"]);
    if (focusedWindowId !== -1) {
      Niri.dispatch(["focus-window", "--id", String(focusedWindowId)]);
    }
  }

  Component.onCompleted: {
    Niri.refreshOutputs();
    Niri.refreshWorkspaces();
    Niri.refreshWindows();
    Qt.callLater(refreshState);
  }

  Connections {
    target: Niri

    function onWorkspacesUpdated() {
      refreshState();
    }

    function onWindowsUpdated() {
      refreshState();
    }

    function onFocusedWindowChanged() {
      refreshState();
    }
  }
}
