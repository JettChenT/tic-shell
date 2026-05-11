import QtQuick

import "." as Widgets

Rectangle {
  id: root

  property var shell
  property var workspace
  readonly property var workspaceWindows: shell.windowStructureRevision >= 0 ? shell.windowsForWorkspace(workspace.id) : []
  readonly property bool current: workspace.id === shell.activeWorkspaceId || workspace.focused || workspace.active || editing
  readonly property int windowListHeight: workspaceWindows.length * 28 + Math.max(0, workspaceWindows.length - 1) * 4
  property bool editing: false

  signal selected(var workspace)
  signal annotationAccepted(int workspaceId, string annotation)
  signal windowSelected(var windowRow)

  width: parent ? parent.width : 0
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
      if (!root.editing && !root.shell.itemContainsScenePoint(annotationInput, eventPoint.scenePosition)) {
        root.selected(root.workspace);
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
        color: root.current ? "#8bd5ca" : "#3b4252"

        Text {
          anchors.centerIn: parent
          color: root.current ? "#181c22" : "#cad3f5"
          font.pixelSize: 13
          font.weight: Font.DemiBold
          text: root.workspace.label
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
        text: root.shell.annotationFor(root.workspace.id)
        clip: true
        selectByMouse: true

        onActiveFocusChanged: {
          root.editing = activeFocus;
          if (!activeFocus) {
            text = root.shell.annotationFor(root.workspace.id);
          }
        }
        onAccepted: {
          root.annotationAccepted(root.workspace.id, text);
          focus = false;
        }
        Keys.onEscapePressed: {
          text = root.shell.annotationFor(root.workspace.id);
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
      visible: root.workspaceWindows.length === 0
      color: "#7f8797"
      font.pixelSize: 12
      verticalAlignment: Text.AlignVCenter
      text: "empty"
    }

    Column {
      id: windowList

      width: parent.width
      height: root.windowListHeight
      spacing: 4
      visible: root.workspaceWindows.length > 0

      Repeater {
        model: root.workspaceWindows

        Widgets.WindowRow {
          shell: root.shell
          windowRow: modelData
          onSelected: windowRow => root.windowSelected(windowRow)
        }
      }
    }
  }
}
