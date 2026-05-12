pragma Singleton

import QtQuick
import qs.Commons

Item {
  id: root

  property bool collapsed: false
  property bool agentPaneCollapsed: true

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
}
