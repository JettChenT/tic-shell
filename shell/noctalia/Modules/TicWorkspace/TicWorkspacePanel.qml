import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Modules.TicWorkspace
import qs.Modules.TicWorkspace as Tic
import qs.Modules.TicWorkspace.Services as TicServices

Item {
  id: root

  property ShellScreen screen
  property string widgetId: ""
  property string section: ""
  property int sectionWidgetIndex: -1
  property int sectionWidgetsCount: 0

  readonly property int workspacePaneWidth: TicWorkspaceState.workspaceWidth()
  readonly property int agentPaneWidth: TicWorkspaceState.agentWidth()
  readonly property int paneDividerWidth: TicWorkspaceState.dividerWidth()
  readonly property int collapsedRailWidth: Style.getBarHeightForScreen(screen?.name)
  readonly property int expandedRailWidth: TicWorkspaceState.expandedWidth()
  readonly property int railWidth: TicWorkspaceState.reservedWidth()
  readonly property string ticShellRoot: Quickshell.env("TIC_SHELL_ROOT") || (Quickshell.env("HOME") + "/dev/tic-shell")
  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/lnx"
  readonly property string stateFile: stateDir + "/workspaces.json"

  readonly property color mPrimary: Color.mPrimary
  readonly property color mOnPrimary: Color.mOnPrimary
  readonly property color mSecondary: Color.mSecondary
  readonly property color mTertiary: Color.mTertiary
  readonly property color mError: Color.mError
  readonly property color mOnError: Color.mOnError
  readonly property color mSurface: Color.mSurface
  readonly property color mOnSurface: Color.mOnSurface
  readonly property color mSurfaceVariant: Color.mSurfaceVariant
  readonly property color mOnSurfaceVariant: Color.mOnSurfaceVariant
  readonly property color mOutline: Color.mOutline
  readonly property color mHover: Color.mHover
  readonly property color mOnHover: Color.mOnHover
  readonly property color barBackground: "transparent"
  readonly property color capsuleColor: Style.capsuleColor
  readonly property color capsuleHoverColor: Qt.alpha(Color.mSecondary, 0.16)

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

  property bool sidebarCollapsed: TicWorkspaceState.collapsed
  property bool agentPaneCollapsed: TicWorkspaceState.agentPaneCollapsed
  property int slashCommandIndex: 0

  implicitWidth: railWidth
  implicitHeight: Math.max(280, Math.min(720, screen ? Math.round(screen.height * 0.58) : 560))
  width: railWidth

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
        result.push({ name: name, description: source[i].description || "" });
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

  function showWindowPreview(windowRow, x, y, rowHeight) {
    workspaceService.showWindowPreview(windowRow, x, y, rowHeight);
  }

  function hideWindowPreview() {
    workspaceService.hideWindowPreview();
  }

  function showSidebar() {
    TicWorkspaceState.collapsed = false;
    scheduleRecenter();
  }

  function hideSidebar() {
    TicWorkspaceState.collapsed = true;
    scheduleRecenter();
  }

  function toggleSidebar() {
    TicWorkspaceState.toggleCollapsed();
    scheduleRecenter();
  }

  function showAgentPane() {
    TicWorkspaceState.agentPaneCollapsed = false;
    scheduleRecenter();
  }

  function hideAgentPane() {
    TicWorkspaceState.agentPaneCollapsed = true;
    scheduleRecenter();
  }

  function toggleAgentPane() {
    TicWorkspaceState.toggleAgentPane();
    scheduleRecenter();
  }

  function scheduleRecenter() {
    recenterTimer.restart();
  }

  Component.onCompleted: {
    Quickshell.execDetached(["mkdir", "-p", stateDir]);
    agentBridge.events = [];
  }

  Connections {
    target: TicWorkspaceState

    function onCollapsedChanged() {
      root.scheduleRecenter();
    }
  }

  TicServices.AnnotationStore {
    id: annotationStore
    stateFile: root.stateFile
  }

  TicServices.WorkspaceService {
    id: workspaceService

    onAgentWorkspaceChanged: {
      root.activeWorkspaceLabel = workspaceService.activeWorkspaceLabel;
      agentBridge.notifyWorkspace();
    }
  }

  TicServices.AgentBridge {
    id: agentBridge

    ticShellRoot: root.ticShellRoot
    workspaceKey: root.currentAgentWorkspaceKey()
    workspaceTitle: root.activeWorkspaceLabel
    onWorkspaceMessage: title => root.activeWorkspaceLabel = title
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
      root.toggleSidebar();
    }

    function reveal() {
      root.showSidebar();
    }

    function hide() {
      root.hideSidebar();
    }

    function toggleAgent() {
      root.toggleAgentPane();
    }

    function revealAgent() {
      root.showAgentPane();
    }

    function hideAgent() {
      root.hideAgentPane();
    }
  }

  Row {
    anchors.fill: parent
    spacing: 0

    Tic.WorkspacePane {
      id: workspacePane
      shell: root
    }

    Rectangle {
      visible: !root.sidebarCollapsed && !root.agentPaneCollapsed
      width: root.paneDividerWidth
      height: parent.height
      color: Color.mOutline
    }

    Tic.AgentPane {
      shell: root
    }
  }
}
