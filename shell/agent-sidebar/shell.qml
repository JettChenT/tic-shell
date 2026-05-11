import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Niri
import Quickshell.Wayland
import Quickshell.Widgets

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

  property var annotations: ({})
  property var agentEvents: []
  property var workspaceRows: []
  property int activeWorkspaceId: -1
  property string activeWorkspaceLabel: "Workspace"
  property bool sidebarCollapsed: false
  property bool agentPaneCollapsed: true
  property bool stateReady: false
  property string agentStatus: "starting"
  property var agentCommands: []
  property int slashCommandIndex: 0

  function workspaceKey(workspaceId) {
    return "niri:workspace:" + workspaceId;
  }

  function windowKey(windowId) {
    return "niri:window:" + windowId;
  }

  function displayWorkspaceName(workspace) {
    if (workspace.name && workspace.name.length > 0) {
      return workspace.name;
    }
    return String(workspace.idx);
  }

  function annotationFor(workspaceId) {
    const entry = annotations[workspaceKey(workspaceId)];
    return entry && entry.annotation ? entry.annotation : "";
  }

  function appendAgentEvent(kind, title, body) {
    const next = agentEvents.slice();
    next.push({
      id: kind + ":" + next.length + ":" + Date.now(),
      kind: kind,
      title: title,
      body: body,
      time: Qt.formatTime(new Date(), "HH:mm")
    });
    agentEvents = next;
  }

  function setAgentEvents(events) {
    agentEvents = events.map((entry, index) => ({
      id: entry.id || ("entry:" + index),
      kind: entry.kind || "system",
      title: entry.title || "Codex",
      body: entry.body || "",
      time: entry.time || Qt.formatTime(new Date(), "HH:mm")
    }));
  }

  function handleAgentLine(line) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      return;
    }

    try {
      const message = JSON.parse(trimmed);
      if (message.type === "status") {
        agentStatus = message.status || "unknown";
      } else if (message.type === "snapshot") {
        setAgentEvents(message.events || []);
      } else if (message.type === "workspace") {
        activeWorkspaceLabel = message.title || activeWorkspaceLabel;
        agentCommands = message.commands || [];
      } else if (message.type === "event") {
        appendAgentEvent(message.kind || "system", message.title || "Codex", message.body || "");
      }
    } catch (error) {
      appendAgentEvent("stderr", "codex-agent", trimmed);
    }
  }

  function sendAgentPrompt(prompt) {
    const trimmed = prompt.trim();
    if (trimmed.length === 0) {
      return;
    }

    if (!codexAgent.running) {
      codexAgent.running = true;
    }

    codexAgent.write(JSON.stringify({
      type: "prompt",
      text: trimmed,
      workspaceKey: currentAgentWorkspaceKey(),
      workspaceTitle: activeWorkspaceLabel
    }) + "\n");
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

  function selectedSlashCommand() {
    const commands = filteredAgentCommands(agentPromptInput.text);
    if (commands.length === 0) {
      return null;
    }
    const index = Math.max(0, Math.min(slashCommandIndex, commands.length - 1));
    return commands[index];
  }

  function completeSlashCommand(command) {
    if (!command) {
      return;
    }
    agentPromptInput.text = "/" + command.name + " ";
    agentPromptInput.cursorPosition = agentPromptInput.text.length;
    agentPromptInput.forceActiveFocus();
  }

  function sendAgentControl(type) {
    if (!codexAgent.running) {
      codexAgent.running = true;
    }

    codexAgent.write(JSON.stringify({
      type: type,
      workspaceKey: currentAgentWorkspaceKey(),
      workspaceTitle: activeWorkspaceLabel
    }) + "\n");
  }

  function currentAgentWorkspaceKey() {
    return activeWorkspaceId === -1 ? "workspace:default" : workspaceKey(activeWorkspaceId);
  }

  function notifyAgentWorkspace() {
    if (!codexAgent.running) {
      return;
    }

    codexAgent.write(JSON.stringify({
      type: "workspace",
      workspaceKey: currentAgentWorkspaceKey(),
      workspaceTitle: activeWorkspaceLabel
    }) + "\n");
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
    let activeWorkspaceId = -1;

    const rows = workspaces.map(ws => {
      const wsWindows = windows.filter(win => win.workspaceId === ws.id);
      return {
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
        activeWindowId: ws.activeWindowId || 0,
        windows: wsWindows
      };
    });

    for (let i = 0; i < rows.length; i++) {
      if (rows[i].focused || rows[i].active) {
        activeWorkspaceId = rows[i].id;
        break;
      }
    }

    if (activeWorkspaceId !== -1) {
      if (activeWorkspaceId !== shell.activeWorkspaceId) {
        shell.activeWorkspaceId = activeWorkspaceId;
        const activeRow = rows.find(row => row.id === activeWorkspaceId);
        activeWorkspaceLabel = activeRow ? activeRow.label : "Workspace";
        notifyAgentWorkspace();
      }
    }

    workspaceRows = rows;
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

  function itemContainsScenePoint(item, scenePoint) {
    const localPoint = item.mapFromItem(null, scenePoint.x, scenePoint.y);
    return localPoint.x >= 0 && localPoint.y >= 0 && localPoint.x <= item.width && localPoint.y <= item.height;
  }

  function focusWindow(windowRow) {
    Niri.dispatch(["focus-window", "--id", String(windowRow.id)]);
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

  function setAnnotation(workspaceId, annotation) {
    const key = workspaceKey(workspaceId);
    const next = Object.assign({}, annotations);
    const trimmed = annotation.trim();

    if (trimmed.length === 0) {
      delete next[key];
    } else {
      next[key] = {
        annotation: trimmed,
        updatedAt: new Date().toISOString()
      };
    }

    annotations = next;
    annotationAdapter.workspaces = annotations;
    annotationFile.writeAdapter();
  }

  Component.onCompleted: {
    Quickshell.execDetached(["mkdir", "-p", stateDir]);
    Niri.refreshOutputs();
    Niri.refreshWorkspaces();
    Niri.refreshWindows();
    agentEvents = [];
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

  Timer {
    id: recenterTimer
    interval: 120
    repeat: false
    onTriggered: Niri.dispatch(["expand-column-to-available-width"])
  }

  FileView {
    id: annotationFile
    path: shell.stateFile
    watchChanges: true
    printErrors: false

    adapter: JsonAdapter {
      id: annotationAdapter
      property var workspaces: ({})
    }

    onLoaded: {
      shell.annotations = annotationAdapter.workspaces || {};
      shell.stateReady = true;
    }

    onLoadFailed: function(error) {
      shell.annotations = {};
      annotationAdapter.workspaces = shell.annotations;
      shell.stateReady = true;
    }
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

  Process {
    id: codexAgent

    command: ["bun", shell.ticShellRoot + "/bin/tic-codex-agent"]
    workingDirectory: Quickshell.env("HOME") || "/home/jettc"
    stdinEnabled: true
    running: false
    environment: ({
      "HOME": Quickshell.env("HOME") || "/home/jettc",
      "PATH": "/run/current-system/sw/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.local/bin:" + (Quickshell.env("HOME") || "/home/jettc") + "/.bun/bin:" + (Quickshell.env("PATH") || ""),
      "TIC_CODEX_WORKDIR": Quickshell.env("HOME") || "/home/jettc"
    })

    stdout: SplitParser {
      onRead: data => shell.handleAgentLine(data)
    }

    stderr: SplitParser {
      onRead: data => {
        const trimmed = data.trim();
        if (trimmed.length > 0) {
          shell.appendAgentEvent("stderr", "codex-agent", trimmed);
        }
      }
    }

    onStarted: shell.agentStatus = "starting"
    onExited: (exitCode, exitStatus) => {
      shell.agentStatus = "stopped";
      shell.appendAgentEvent("system", "Codex agent stopped", "exit " + exitCode);
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

        Item {
          id: workspacePane

          width: shell.sidebarCollapsed ? shell.collapsedRailWidth : shell.workspacePaneWidth
          height: parent.height

          Column {
            anchors.fill: parent
            anchors.margins: shell.sidebarCollapsed ? 6 : 12
            spacing: 10

            Row {
              width: parent.width
              height: 32
              spacing: shell.sidebarCollapsed ? 0 : 8

              Rectangle {
                id: collapseSidebarButton
                width: 32
                height: 32
                radius: 6
                color: collapseSidebarMouse.containsMouse ? "#3a4050" : "#2b303b"
                border.color: "#596173"

                Text {
                  anchors.centerIn: parent
                  color: "#8bd5ca"
                  font.pixelSize: 18
                  font.weight: Font.DemiBold
                  text: shell.sidebarCollapsed ? ">" : "<"
                }

                MouseArea {
                  id: collapseSidebarMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  onClicked: shell.toggleSidebar()
                }
              }

              Text {
                visible: !shell.sidebarCollapsed
                width: parent.width - collapseSidebarButton.width - addWorkspaceButton.width - toggleAgentPaneButton.width - parent.spacing * 3
                height: parent.height
                color: "#cad3f5"
                font.pixelSize: 17
                font.weight: Font.DemiBold
                verticalAlignment: Text.AlignVCenter
                text: "Workspaces"
                elide: Text.ElideRight
              }

              Rectangle {
                id: addWorkspaceButton
                visible: !shell.sidebarCollapsed
                width: 32
                height: 32
                radius: 6
                color: addWorkspaceMouse.containsMouse ? "#3a4050" : "#2b303b"
                border.color: "#596173"

                Text {
                  anchors.centerIn: parent
                  color: "#8bd5ca"
                  font.pixelSize: 22
                  text: "+"
                }

                MouseArea {
                  id: addWorkspaceMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  onClicked: shell.focusBottomWorkspace()
                }
              }

              Rectangle {
                id: toggleAgentPaneButton
                visible: !shell.sidebarCollapsed
                width: 32
                height: 32
                radius: 6
                color: toggleAgentPaneMouse.containsMouse ? "#3a4050" : "#2b303b"
                border.color: shell.agentPaneCollapsed ? "#596173" : "#8bd5ca"

                Text {
                  anchors.centerIn: parent
                  color: shell.agentPaneCollapsed ? "#7f8797" : "#8bd5ca"
                  font.pixelSize: 13
                  font.weight: Font.DemiBold
                  text: "C"
                }

                MouseArea {
                  id: toggleAgentPaneMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  onClicked: shell.toggleAgentPane()
                }
              }
            }

            Flickable {
              id: workspaceScroller
              visible: !shell.sidebarCollapsed
              width: parent.width
              height: parent.height - y
              clip: true
              contentWidth: width
              contentHeight: workspaceColumn.height

              Column {
                id: workspaceColumn
                width: workspaceScroller.width
                spacing: 8

                Repeater {
                  model: shell.workspaceRows

                  Rectangle {
                    id: card

                readonly property var workspace: modelData
                readonly property var workspaceWindows: workspace.windows || []
                readonly property bool current: workspace.id === shell.activeWorkspaceId || workspace.focused || workspace.active || editing
                readonly property int windowListHeight: workspaceWindows.length * 28 + Math.max(0, workspaceWindows.length - 1) * 4
                property bool editing: false

                width: workspaceColumn.width
                height: 58 + (workspaceWindows.length > 0 ? windowListHeight + 8 : emptyWorkspaceLabel.height + 7)
                radius: 7
                color: current ? "#334044" : (cardHover.hovered ? "#2d3340" : "#252a34")
                border.color: workspace.urgent ? "#ed8796" : (current ? "#8bd5ca" : "#3a4050")
                border.width: current ? 2 : 1

                HoverHandler {
                  id: cardHover
                }

                TapHandler {
                  acceptedButtons: Qt.LeftButton
                  gesturePolicy: TapHandler.ReleaseWithinBounds
                  onTapped: function(eventPoint) {
                    if (!card.editing && !shell.itemContainsScenePoint(annotationInput, eventPoint.scenePosition)) {
                      shell.focusWorkspace(card.workspace);
                    }
                  }
                }

                Column {
                  anchors.fill: parent
                  anchors.margins: 10
                  spacing: 7

                  Row {
                    width: parent.width
                    height: 25
                    spacing: 8

                    Rectangle {
                      width: 30
                      height: 24
                      radius: 6
                      color: current ? "#8bd5ca" : "#3b4252"

                      Text {
                        anchors.centerIn: parent
                        color: current ? "#181c22" : "#cad3f5"
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        text: workspace.label
                      }
                    }

                    TextInput {
                      id: annotationInput
                      width: parent.width - 38
                      height: 25
                      color: activeFocus ? "#ffffff" : (text.length > 0 ? "#cad3f5" : "#7f8797")
                      selectedTextColor: "#181c22"
                      selectionColor: "#8bd5ca"
                      font.pixelSize: 14
                      font.weight: text.length > 0 ? Font.DemiBold : Font.Normal
                      verticalAlignment: TextInput.AlignVCenter
                      text: shell.annotationFor(workspace.id)
                      clip: true
                      selectByMouse: true
                      onActiveFocusChanged: {
                        card.editing = activeFocus;
                        if (!activeFocus) {
                          text = shell.annotationFor(workspace.id);
                        }
                      }
                      onAccepted: {
                        shell.setAnnotation(workspace.id, text);
                        focus = false;
                      }
                      Keys.onEscapePressed: {
                        text = shell.annotationFor(workspace.id);
                        focus = false;
                      }

                      Text {
                        anchors.fill: parent
                        visible: annotationInput.text.length === 0 && !annotationInput.activeFocus
                        color: "#697284"
                        font.pixelSize: 14
                        verticalAlignment: Text.AlignVCenter
                        text: "name workspace"
                        elide: Text.ElideRight
                      }
                    }
                  }

                  Text {
                    id: emptyWorkspaceLabel
                    width: parent.width
                    height: 20
                    visible: card.workspaceWindows.length === 0
                    color: "#7f8797"
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    text: "empty"
                  }

                  Column {
                    id: windowList
                    width: parent.width
                    height: card.windowListHeight
                    spacing: 4
                    visible: card.workspaceWindows.length > 0

                    Repeater {
                      model: card.workspaceWindows

	                      Rectangle {
	                        id: windowIconRow

	                        readonly property var win: modelData
	                        readonly property string iconPath: shell.iconForAppId(win.appId)

                        width: windowList.width
                        height: 28
                        radius: 5
                        color: win.focused ? "#3d4b4f" : (windowHover.hovered ? "#303642" : "#272d37")
                        border.color: win.focused ? "#8bd5ca" : "#3a4050"

                        Row {
                          anchors.fill: parent
                          anchors.leftMargin: 8
                          anchors.rightMargin: 8
                          spacing: 7

	                          Item {
	                            id: windowIcon

	                            width: 18
	                            height: parent.height

	                            IconImage {
	                              anchors.centerIn: parent
	                              width: 16
	                              height: 16
	                              source: windowIconRow.iconPath
	                              visible: windowIconRow.iconPath.length > 0
	                              mipmap: true
	                            }

	                            Text {
	                              anchors.fill: parent
	                              visible: windowIconRow.iconPath.length === 0
	                              color: "#a6da95"
	                              font.pixelSize: 12
                              font.weight: Font.DemiBold
                              horizontalAlignment: Text.AlignHCenter
                              verticalAlignment: Text.AlignVCenter
                              text: shell.appInitial(win.appId)
                            }
                          }

                          Text {
                            width: parent.width - 25
                            height: parent.height
                            color: win.focused ? "#ffffff" : "#b8c0d6"
                            font.pixelSize: 12
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                            text: win.title
                          }
                        }

                        HoverHandler {
                          id: windowHover
                        }

                        TapHandler {
                          acceptedButtons: Qt.LeftButton
                          gesturePolicy: TapHandler.ReleaseWithinBounds
                          onTapped: shell.focusWindow(win)
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }

          }
        }

        Rectangle {
          visible: !shell.sidebarCollapsed && !shell.agentPaneCollapsed
          width: shell.paneDividerWidth
          height: parent.height
          color: "#3a4050"
        }

        Item {
          id: agentPane

          visible: !shell.sidebarCollapsed && !shell.agentPaneCollapsed
          width: shell.agentPaneWidth
          height: parent.height

          Column {
            anchors.fill: parent
            anchors.margins: 12
            spacing: 10

            Column {
              width: parent.width
              height: 34
              spacing: 3

              Row {
                width: parent.width
                height: 28
                spacing: 6

                Text {
                  width: parent.width - newSessionButton.width - clearSessionButton.width - cancelSessionButton.width - parent.spacing * 3
                  height: parent.height
                  color: "#cad3f5"
                  font.pixelSize: 17
                  font.weight: Font.DemiBold
                  verticalAlignment: Text.AlignVCenter
                  text: "Codex"
                  elide: Text.ElideRight
                }

                Rectangle {
                  id: newSessionButton
                  width: 28
                  height: 28
                  radius: 6
                  color: newSessionMouse.containsMouse ? "#3a4050" : "#2b303b"
                  border.color: "#596173"

                  Text {
                    anchors.centerIn: parent
                    color: "#8bd5ca"
                    font.pixelSize: 18
                    text: "+"
                  }

                  MouseArea {
                    id: newSessionMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: shell.sendAgentControl("new")
                  }
                }

                Rectangle {
                  id: clearSessionButton
                  width: 28
                  height: 28
                  radius: 6
                  color: clearSessionMouse.containsMouse ? "#3a4050" : "#2b303b"
                  border.color: "#596173"

                  Text {
                    anchors.centerIn: parent
                    color: "#cad3f5"
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    text: "C"
                  }

                  MouseArea {
                    id: clearSessionMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: shell.sendAgentControl("clear")
                  }
                }

                Rectangle {
                  id: cancelSessionButton
                  width: 28
                  height: 28
                  radius: 6
                  color: cancelSessionMouse.containsMouse ? "#3a4050" : "#2b303b"
                  border.color: "#596173"

                  Text {
                    anchors.centerIn: parent
                    color: "#ed8796"
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    text: "x"
                  }

                  MouseArea {
                    id: cancelSessionMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: shell.sendAgentControl("cancel")
                  }
                }
              }

              Text {
                visible: shell.agentStatus === "error" || shell.agentStatus === "stopped"
                width: parent.width
                height: 16
                color: shell.agentStatus === "error" || shell.agentStatus === "stopped" ? "#ed8796" : "#8bd5ca"
                font.pixelSize: 12
                text: shell.agentStatus === "error" || shell.agentStatus === "stopped" ? shell.agentStatus : ""
                elide: Text.ElideRight
              }

            }

            Flickable {
              id: agentTranscript
              width: parent.width
              height: parent.height - y - agentInputBox.height - 10
              clip: true
              contentWidth: width
              contentHeight: agentEventColumn.height

              Column {
                id: agentEventColumn
                width: agentTranscript.width
                spacing: 8

                Repeater {
                  model: shell.agentEvents

                  Rectangle {
                    readonly property bool isUser: modelData.kind === "user"
                    readonly property bool isAssistant: modelData.kind === "assistant"
                    readonly property bool isThought: modelData.kind === "thought"
                    readonly property bool isThinking: modelData.kind === "thinking"
                    readonly property bool isTool: modelData.kind === "tool"
                    readonly property bool isPermission: modelData.kind === "permission"
                    readonly property bool hasHeader: isTool || isPermission || isThinking || isThought

                    width: parent.width
                    height: Math.max(isThinking ? 38 : 46, eventBody.implicitHeight + (hasHeader ? 36 : 20))
                    radius: 7
                    color: isTool ? "#26333b" : (isPermission ? "#332f3c" : (isUser ? "#303642" : (isThinking ? "#222832" : "#20242c")))
                    border.color: isTool ? "#8aadf4" : (isPermission ? "#c6a0f6" : (isThinking ? "#596173" : "transparent"))
                    border.width: hasHeader || isUser ? 1 : 0

                    Column {
                      anchors.fill: parent
                      anchors.margins: 9
                      spacing: 5

                      Row {
                        visible: parent.parent.hasHeader
                        width: parent.width
                        height: 15
                        spacing: 6

                        Text {
                          width: parent.width - eventTime.width - parent.spacing
                          height: parent.height
                          color: isTool ? "#8aadf4" : (isPermission ? "#c6a0f6" : (isThinking ? "#eed49f" : "#a6da95"))
                          font.pixelSize: 12
                          font.weight: Font.DemiBold
                          text: modelData.title
                          elide: Text.ElideRight
                        }

                        Text {
                          id: eventTime
                          visible: !parent.parent.parent.isThinking
                          width: 38
                          height: parent.height
                          color: "#7f8797"
                          font.pixelSize: 11
                          horizontalAlignment: Text.AlignRight
                          text: modelData.time
                        }
                      }

                      Text {
                        id: eventBody
                        visible: modelData.kind !== "thinking" || modelData.body.length > 0
                        width: parent.width
                        color: "#b8c0d6"
                        font.pixelSize: 12
                        wrapMode: Text.Wrap
                        text: modelData.body
                      }
                    }
                  }
                }
              }
            }

            Rectangle {
              id: agentInputBox
              width: parent.width
              height: 76
              z: 10
              radius: 7
              color: "#252a34"
              border.color: agentPromptInput.activeFocus ? "#8bd5ca" : "#3a4050"

              Rectangle {
                id: slashCommandPopup

                readonly property var commands: shell.filteredAgentCommands(agentPromptInput.text)

                visible: agentPromptInput.activeFocus && commands.length > 0
                width: parent.width
                height: visible ? Math.min(166, commands.length * 38 + 10) : 0
                x: 0
                y: -height - 6
                z: 20
                radius: 7
                color: "#252a34"
                border.color: "#596173"
                clip: true

                onCommandsChanged: {
                  if (shell.slashCommandIndex >= commands.length) {
                    shell.slashCommandIndex = Math.max(0, commands.length - 1);
                  }
                  slashCommandList.positionViewAtIndex(shell.slashCommandIndex, ListView.Contain);
                }

                ListView {
                  id: slashCommandList

                  anchors.fill: parent
                  anchors.margins: 5
                  clip: true
                  spacing: 3
                  model: slashCommandPopup.commands
                  currentIndex: shell.slashCommandIndex
                  boundsBehavior: Flickable.StopAtBounds

                  onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

                  delegate: Rectangle {
                    readonly property bool selected: index === shell.slashCommandIndex

                    width: slashCommandList.width
                    height: 35
                    radius: 5
                    color: selected || slashCommandMouse.containsMouse ? "#303642" : "transparent"

                    Row {
                      anchors.fill: parent
                      anchors.leftMargin: 8
                      anchors.rightMargin: 8
                      spacing: 8

                      Text {
                        width: 86
                        height: parent.height
                        color: "#8bd5ca"
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                        verticalAlignment: Text.AlignVCenter
                        text: "/" + modelData.name
                        elide: Text.ElideRight
                      }

                      Text {
                        width: parent.width - 94
                        height: parent.height
                        color: "#b8c0d6"
                        font.pixelSize: 11
                        verticalAlignment: Text.AlignVCenter
                        text: modelData.description
                        elide: Text.ElideRight
                      }
                    }

                    MouseArea {
                      id: slashCommandMouse
                      anchors.fill: parent
                      hoverEnabled: true
                      onEntered: shell.slashCommandIndex = index
                      onClicked: shell.completeSlashCommand(modelData)
                    }
                  }
                }

                Rectangle {
                  visible: slashCommandList.contentHeight > slashCommandList.height
                  width: 3
                  height: Math.max(18, slashCommandList.height * slashCommandList.height / slashCommandList.contentHeight)
                  x: parent.width - width - 3
                  y: 5 + (slashCommandList.height - height) * (slashCommandList.contentY / Math.max(1, slashCommandList.contentHeight - slashCommandList.height))
                  radius: 2
                  color: "#7f8797"
                  opacity: 0.75
                }
              }

              Column {
                anchors.fill: parent
                anchors.margins: 9
                spacing: 7

                TextInput {
                  id: agentPromptInput
                  width: parent.width
                  height: 26
                  color: "#ffffff"
                  selectedTextColor: "#181c22"
                  selectionColor: "#8bd5ca"
                  font.pixelSize: 13
                  clip: true
                  selectByMouse: true
                  verticalAlignment: TextInput.AlignVCenter
                  onTextChanged: {
                    if (!text.startsWith("/")) {
                      shell.slashCommandIndex = 0;
                    } else {
                      const commands = shell.filteredAgentCommands(text);
                      if (shell.slashCommandIndex >= commands.length) {
                        shell.slashCommandIndex = Math.max(0, commands.length - 1);
                      }
                    }
                  }
                  onAccepted: {
                    if (slashCommandPopup.visible && shell.selectedSlashCommand() !== null && text.indexOf(" ") === -1) {
                      shell.completeSlashCommand(shell.selectedSlashCommand());
                    } else {
                      shell.sendAgentPrompt(text);
                      text = "";
                    }
                  }

                  Keys.onDownPressed: function(event) {
                    const commands = shell.filteredAgentCommands(text);
                    if (commands.length > 0) {
                      shell.slashCommandIndex = Math.min(commands.length - 1, shell.slashCommandIndex + 1);
                      slashCommandList.positionViewAtIndex(shell.slashCommandIndex, ListView.Contain);
                      event.accepted = true;
                    }
                  }

                  Keys.onUpPressed: function(event) {
                    const commands = shell.filteredAgentCommands(text);
                    if (commands.length > 0) {
                      shell.slashCommandIndex = Math.max(0, shell.slashCommandIndex - 1);
                      slashCommandList.positionViewAtIndex(shell.slashCommandIndex, ListView.Contain);
                      event.accepted = true;
                    }
                  }

                  Keys.onEscapePressed: function(event) {
                    if (slashCommandPopup.visible) {
                      text = "";
                      shell.slashCommandIndex = 0;
                      event.accepted = true;
                    }
                  }

                  Keys.onTabPressed: function(event) {
                    if (slashCommandPopup.visible && shell.selectedSlashCommand() !== null) {
                      shell.completeSlashCommand(shell.selectedSlashCommand());
                      event.accepted = true;
                    }
                  }

                  Text {
                    anchors.fill: parent
                    visible: agentPromptInput.text.length === 0 && !agentPromptInput.activeFocus
                    color: "#697284"
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                    text: "ask Codex"
                    elide: Text.ElideRight
                  }
                }

                Row {
                  width: parent.width
                  height: 24
                  spacing: 8

                  Text {
                    width: parent.width - sendPromptButton.width - parent.spacing
                    height: parent.height
                    color: "#7f8797"
                    font.pixelSize: 11
                    verticalAlignment: Text.AlignVCenter
                    text: "All actions allowed"
                    elide: Text.ElideRight
                  }

                  Rectangle {
                    id: sendPromptButton
                    width: 64
                    height: 24
                    radius: 6
                    color: sendPromptMouse.containsMouse ? "#3d4b4f" : "#2b303b"
                    border.color: "#8bd5ca"

                    Text {
                      anchors.centerIn: parent
                      color: "#cad3f5"
                      font.pixelSize: 12
                      font.weight: Font.DemiBold
                      text: "Send"
                    }

                    MouseArea {
                      id: sendPromptMouse
                      anchors.fill: parent
                      hoverEnabled: true
                      onClicked: {
                        shell.sendAgentPrompt(agentPromptInput.text);
                        agentPromptInput.text = "";
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
