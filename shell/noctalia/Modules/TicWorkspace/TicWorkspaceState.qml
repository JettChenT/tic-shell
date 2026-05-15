pragma Singleton

import QtQuick
import qs.Commons

Item {
  id: root

  property bool collapsed: false
  property bool agentPaneCollapsed: true
  property bool debugPaneOpen: false
  property var windowDescriptions: ({})
  property int windowDescriptionRevision: 0

  function workspaceWidth() {
    return Settings.data.bar.ticWorkspaceWidth || 250;
  }

  function agentWidth() {
    return Settings.data.bar.ticAgentPaneWidth || 360;
  }

  function dividerWidth() {
    return 1;
  }

  function expandedWidth() {
    return workspaceWidth() + (agentPaneCollapsed ? 0 : dividerWidth() + agentWidth());
  }

  function reservedWidth() {
    return collapsed ? 0 : expandedWidth();
  }

  function toggleCollapsed() {
    collapsed = !collapsed;
  }

  function toggleAgentPane() {
    agentPaneCollapsed = !agentPaneCollapsed;
  }

  function showDebugPane() {
    collapsed = false;
    agentPaneCollapsed = true;
    debugPaneOpen = true;
  }

  function hideDebugPane() {
    debugPaneOpen = false;
  }

  function toggleDebugPane() {
    if (debugPaneOpen) {
      hideDebugPane();
    } else {
      showDebugPane();
    }
  }

  function setWindowDescription(windowId, description) {
    const key = String(windowId || "");
    if (key.length === 0) {
      return;
    }

    const next = Object.assign({}, windowDescriptions);
    const text = String(description || "");
    if (text.length > 0) {
      next[key] = {
        description: text,
        updatedAt: new Date().toISOString()
      };
    } else {
      delete next[key];
    }

    windowDescriptions = next;
    windowDescriptionRevision++;
  }

  function windowDescriptionFor(windowId, revision) {
    const entry = windowDescriptions[String(windowId)];
    return entry && entry.description ? entry.description : "";
  }
}
