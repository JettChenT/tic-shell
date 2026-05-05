import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Niri
import Quickshell.Wayland
import Quickshell.Widgets

ShellRoot {
  id: shell

  readonly property int railWidth: 250
  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/lnx"
  readonly property string stateFile: stateDir + "/workspaces.json"

  property var annotations: ({})
  property var workspaceRows: []
  property var windowRows: []
  property int expandedWorkspaceId: -1
  property bool stateReady: false

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

  function windowsForWorkspace(workspaceId) {
    return windowRows.filter(win => win.workspaceId === workspaceId);
  }

  function appIdsForWorkspace(workspaceId) {
    return appIdsForWindows(windowsForWorkspace(workspaceId));
  }

  function appIdsForWindows(wins) {
    const seen = {};
    const apps = [];
    for (let i = 0; i < wins.length; i++) {
      const appId = wins[i].appId || "app";
      if (!seen[appId]) {
        seen[appId] = true;
        apps.push(appId);
      }
    }
    return apps;
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
        windows: wsWindows,
        appIds: appIdsForWindows(wsWindows)
      };
    });

    for (let i = 0; i < rows.length; i++) {
      if (rows[i].focused || rows[i].active) {
        activeWorkspaceId = rows[i].id;
        break;
      }
    }

    if (activeWorkspaceId !== -1) {
      expandedWorkspaceId = activeWorkspaceId;
    }

    windowRows = windows;
    workspaceRows = rows;
  }

  function nextWorkspaceIndex() {
    let maxIdx = 0;
    for (let i = 0; i < workspaceRows.length; i++) {
      maxIdx = Math.max(maxIdx, workspaceRows[i].idx);
    }
    return maxIdx + 1;
  }

  function focusWorkspace(workspace) {
    Niri.dispatch(["focus-workspace", String(workspace.idx)]);
  }

  function focusWindow(windowRow) {
    Niri.dispatch(["focus-window", "--id", String(windowRow.id)]);
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

  PanelWindow {
    id: panel

    color: "#20242c"
    implicitWidth: shell.railWidth

    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "tic-shell-agent-sidebar"
    WlrLayershell.exclusionMode: ExclusionMode.Auto
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

      Column {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10

        Row {
          width: parent.width
          height: 32
          spacing: 8

          Text {
            width: parent.width - addWorkspaceButton.width - parent.spacing
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
              onClicked: Niri.dispatch(["focus-workspace", String(shell.nextWorkspaceIndex())])
            }
          }
        }

        Flickable {
          id: workspaceScroller
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
                readonly property var appIds: workspace.appIds || []
                readonly property bool expanded: workspace.id === shell.expandedWorkspaceId && workspaceWindows.length > 0
                readonly property int expandedWindowListHeight: workspaceWindows.length * 28 + Math.max(0, workspaceWindows.length - 1) * 4
                property bool editing: false

                width: workspaceColumn.width
                height: 58 + (appBadgeRow.visible ? appBadgeRow.height + 7 : 0) + (expanded ? expandedWindowListHeight + 8 : 0)
                radius: 7
                color: workspace.focused ? "#334044" : (cardMouse.containsMouse ? "#2d3340" : "#252a34")
                border.color: workspace.urgent ? "#ed8796" : (workspace.focused ? "#8bd5ca" : "#3a4050")
                border.width: workspace.focused ? 2 : 1

                MouseArea {
                  id: cardMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  acceptedButtons: Qt.LeftButton
                  onClicked: {
                    if (!card.editing) {
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
                      color: workspace.focused ? "#8bd5ca" : "#3b4252"

                      Text {
                        anchors.centerIn: parent
                        color: workspace.focused ? "#181c22" : "#cad3f5"
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

                  Row {
                    id: appBadgeRow
                    width: parent.width
                    height: 20
                    spacing: 5
                    visible: !card.expanded

                    Repeater {
                      model: card.appIds

                      Rectangle {
                        readonly property string iconPath: shell.iconForAppId(modelData)

                        width: 20
                        height: 20
                        radius: 5
                        color: "#3a4050"
                        border.color: "#596173"

                        IconImage {
                          anchors.centerIn: parent
                          width: 14
                          height: 14
                          source: parent.iconPath
                          visible: parent.iconPath.length > 0
                          mipmap: true
                        }

                        Text {
                          anchors.centerIn: parent
                          visible: parent.iconPath.length === 0
                          color: "#a6da95"
                          font.pixelSize: 11
                          font.weight: Font.DemiBold
                          text: shell.appInitial(modelData)
                        }
                      }
                    }

                    Text {
                      height: parent.height
                      color: "#7f8797"
                      font.pixelSize: 12
                      verticalAlignment: Text.AlignVCenter
                      text: card.workspaceWindows.length === 0 ? "empty" : card.workspaceWindows.length + " window" + (card.workspaceWindows.length === 1 ? "" : "s")
                    }
                  }

                  Column {
                    id: windowList
                    width: parent.width
                    height: card.expandedWindowListHeight
                    spacing: 4
                    visible: card.expanded

                    Repeater {
                      model: card.workspaceWindows

	                      Rectangle {
	                        id: windowIconRow

	                        readonly property var win: modelData
	                        readonly property string iconPath: shell.iconForAppId(win.appId)

                        width: windowList.width
                        height: 28
                        radius: 5
                        color: win.focused ? "#3d4b4f" : (windowMouse.containsMouse ? "#303642" : "#272d37")
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

                        MouseArea {
                          id: windowMouse
                          anchors.fill: parent
                          hoverEnabled: true
                          onClicked: shell.focusWindow(win)
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
}
