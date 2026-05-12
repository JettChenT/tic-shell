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
  signal windowPreviewRequested(var windowRow, real x, real y, real height)
  signal windowPreviewHidden()

  width: parent ? parent.width : 0
  height: 58 + (workspaceWindows.length > 0 ? windowListHeight + 8 : emptyWorkspaceLabel.height + 7)
  radius: 16
  color: cardHover.hovered ? Qt.alpha(root.shell.mSecondary, 0.14) : root.shell.capsuleColor
  border.color: workspace.urgent ? root.shell.mError : (current ? root.shell.mPrimary : root.shell.mOutline)
  border.width: 1

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
    anchors.margins: 9
    spacing: 7

    Row {
      width: parent.width
      height: 25
      spacing: 8

      Rectangle {
        width: 30
        height: 24
        radius: 12
        color: root.current ? root.shell.mPrimary : Qt.alpha(root.shell.mOnSurfaceVariant, 0.18)

        Text {
          anchors.centerIn: parent
          color: root.current ? root.shell.mOnPrimary : root.shell.mOnSurface
          font.pixelSize: 13
          font.weight: Font.DemiBold
          text: root.workspace.label
        }
      }

      TextInput {
        id: annotationInput

        width: parent.width - 38
        height: 25
        color: activeFocus ? root.shell.mOnSurface : (text.length > 0 ? root.shell.mOnSurface : root.shell.mOnSurfaceVariant)
        selectedTextColor: root.shell.mOnPrimary
        selectionColor: root.shell.mPrimary
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
          color: root.shell.mOnSurfaceVariant
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
      color: root.shell.mOnSurfaceVariant
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
          onPreviewRequested: (windowRow, x, y, height) => root.windowPreviewRequested(windowRow, x, y, height)
          onPreviewHidden: root.windowPreviewHidden()
        }
      }
    }
  }
}
