import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

import "Modules" as Modules
import "Services" as Services

ShellRoot {
  id: shell

  readonly property int workspacePaneWidth: 250
  readonly property int agentPaneWidth: 360
  readonly property int paneDividerWidth: 1
  readonly property int expandedRailWidth: workspacePaneWidth + (agentPaneCollapsed ? 0 : paneDividerWidth + agentPaneWidth)
  readonly property int railWidth: sidebarCollapsed ? collapsedRailWidth : expandedRailWidth
  readonly property int collapsedRailWidth: 44
  readonly property string ticShellRoot: Quickshell.env("TIC_SHELL_ROOT") || (Quickshell.env("HOME") + "/dev/tic-shell")
  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/lnx"
  readonly property string stateFile: stateDir + "/workspaces.json"

  readonly property var annotations: annotationStore.annotations
  readonly property var agentEvents: agentBridge.events
  readonly property var workspaceRows: workspaceService.workspaceRows
  readonly property var windowRows: workspaceService.windowRows
  readonly property int windowRevision: workspaceService.windowRevision
  readonly property var windowStructureRows: workspaceService.windowStructureRows
  readonly property int windowStructureRevision: workspaceService.windowStructureRevision
  readonly property int activeWorkspaceId: workspaceService.activeWorkspaceId
  property string activeWorkspaceLabel: workspaceService.activeWorkspaceLabel
  readonly property bool stateReady: annotationStore.ready
  readonly property string agentStatus: agentBridge.status
  readonly property var agentCommands: agentBridge.commands

  property bool sidebarCollapsed: false
  property bool agentPaneCollapsed: true
  property int slashCommandIndex: 0

  function annotationFor(workspaceId) {
    return annotationStore.annotationFor(workspaceId);
  }

  function setAnnotation(workspaceId, annotation) {
    annotationStore.setAnnotation(workspaceId, annotation);
  }

  function sendAgentPrompt(prompt) {
    agentBridge.sendPrompt(prompt);
  }

  function sendAgentControl(type) {
    agentBridge.sendControl(type);
  }

  function builtInAgentCommands() {
    return [
      { name: "clear", description: "Clear this workspace session" },
      { name: "new", description: "Start a new session for this workspace" },
      { name: "cancel", description: "Cancel the running turn" },
      { name: "help", description: "Show available slash commands" }
    ];
  }

  function allAgentCommands() {
    const seen = {};
    const result = [];
    const source = builtInAgentCommands().concat(agentCommands || []);
    for (let i = 0; i < source.length; i++) {
      const name = source[i].name || "";
      if (name.length > 0 && !seen[name]) {
        seen[name] = true;
        result.push({
          name: name,
          description: source[i].description || ""
        });
      }
    }
    return result;
  }

  function slashCommandQuery(text) {
    const trimmed = text || "";
    if (!trimmed.startsWith("/") || trimmed.indexOf(" ") !== -1) {
      return "";
    }
    return trimmed.substring(1).toLowerCase();
  }

  function filteredAgentCommands(text) {
    if (!(text || "").startsWith("/") || (text || "").indexOf(" ") !== -1) {
      return [];
    }

    const query = slashCommandQuery(text);
    return allAgentCommands().filter(command => {
      const name = command.name.toLowerCase();
      const description = (command.description || "").toLowerCase();
      return query.length === 0 || name.indexOf(query) !== -1 || description.indexOf(query) !== -1;
    });
  }

  function selectedSlashCommand(text) {
    const commands = filteredAgentCommands(text || "");
    if (commands.length === 0) {
      return null;
    }
    const index = Math.max(0, Math.min(slashCommandIndex, commands.length - 1));
    return commands[index];
  }

  function currentAgentWorkspaceKey() {
    return workspaceService.currentAgentWorkspaceKey();
  }

  function windowsForWorkspace(workspaceId) {
    return workspaceService.windowsForWorkspace(workspaceId);
  }

  function windowTitle(windowId, revision) {
    return workspaceService.windowTitle(windowId, revision);
  }

  function windowFocused(windowId, revision) {
    return workspaceService.windowFocused(windowId, revision);
  }

  function appInitial(appId) {
    return workspaceService.appInitial(appId);
  }

  function iconForAppId(appId) {
    return workspaceService.iconForAppId(appId);
  }

  function focusBottomWorkspace() {
    workspaceService.focusBottomWorkspace();
  }

  function itemContainsScenePoint(item, scenePoint) {
    const localPoint = item.mapFromItem(null, scenePoint.x, scenePoint.y);
    return localPoint.x >= 0 && localPoint.y >= 0 && localPoint.x <= item.width && localPoint.y <= item.height;
  }

  function focusWorkspace(workspace) {
    workspaceService.focusWorkspace(workspace);
  }

  function focusWindow(windowRow) {
    workspaceService.focusWindow(windowRow);
  }

  function showSidebar() {
    sidebarCollapsed = false;
    scheduleRecenter();
  }

  function hideSidebar() {
    sidebarCollapsed = true;
    scheduleRecenter();
  }

  function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
    scheduleRecenter();
  }

  function showAgentPane() {
    agentPaneCollapsed = false;
    scheduleRecenter();
  }

  function hideAgentPane() {
    agentPaneCollapsed = true;
    scheduleRecenter();
  }

  function toggleAgentPane() {
    agentPaneCollapsed = !agentPaneCollapsed;
    scheduleRecenter();
  }

  function scheduleRecenter() {
    recenterTimer.restart();
  }

  Component.onCompleted: {
    Quickshell.execDetached(["mkdir", "-p", stateDir]);
    agentBridge.events = [];
  }

  Services.AnnotationStore {
    id: annotationStore

    stateFile: shell.stateFile
  }

  Services.WorkspaceService {
    id: workspaceService

    onAgentWorkspaceChanged: {
      shell.activeWorkspaceLabel = workspaceService.activeWorkspaceLabel;
      agentBridge.notifyWorkspace();
    }
  }

  Services.AgentBridge {
    id: agentBridge

    ticShellRoot: shell.ticShellRoot
    workspaceKey: shell.currentAgentWorkspaceKey()
    workspaceTitle: shell.activeWorkspaceLabel
    onWorkspaceMessage: title => shell.activeWorkspaceLabel = title
  }

  Timer {
    id: recenterTimer

    interval: 120
    repeat: false
    onTriggered: workspaceService.recenterColumns()
  }

  IpcHandler {
    target: "sidebar"

    function toggle() {
      shell.toggleSidebar();
    }

    function reveal() {
      shell.showSidebar();
    }

    function hide() {
      shell.hideSidebar();
    }

    function toggleAgent() {
      shell.toggleAgentPane();
    }

    function revealAgent() {
      shell.showAgentPane();
    }

    function hideAgent() {
      shell.hideAgentPane();
    }
  }

  PanelWindow {
    id: panel

    color: "#20242c"
    implicitWidth: shell.railWidth

    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "tic-shell-agent-sidebar"
    WlrLayershell.exclusionMode: ExclusionMode.Normal
    WlrLayershell.exclusiveZone: shell.railWidth
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

    anchors {
      top: true
      bottom: true
      left: true
    }

    Rectangle {
      anchors.fill: parent
      color: "#20242c"
      border.color: "#8bd5ca"
      border.width: 1

      Row {
        anchors.fill: parent
        spacing: 0

        Modules.WorkspacePane {
          shell: shell
        }

        Rectangle {
          visible: !shell.sidebarCollapsed && !shell.agentPaneCollapsed
          width: shell.paneDividerWidth
          height: parent.height
          color: "#3a4050"
        }

        Modules.AgentPane {
          shell: shell
        }
      }
    }
  }
}
