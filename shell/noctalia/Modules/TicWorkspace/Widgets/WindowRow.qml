import QtQuick
import Quickshell.Widgets

Rectangle {
  id: root

  property var shell
  property var windowRow
  readonly property string iconPath: shell ? shell.iconForAppId(windowRow.appId) : ""
  readonly property bool windowFocused: shell ? shell.windowFocused(windowRow.id, shell.windowRevision) : false

  signal selected(var windowRow)
  signal previewRequested(var windowRow, real x, real y, real height)
  signal previewHidden()

  width: parent ? parent.width : 0
  height: 28
  radius: 12
  color: windowFocused ? Qt.alpha(root.shell.mHover, 0.28) : (windowHover.hovered ? Qt.alpha(root.shell.mHover, 0.18) : Qt.alpha(root.shell.mSurface, 0.35))
  border.color: windowFocused ? root.shell.mHover : root.shell.mOutline

  Row {
    anchors.fill: parent
    anchors.leftMargin: 8
    anchors.rightMargin: 8
    spacing: 7

    Item {
      width: 18
      height: parent.height

      IconImage {
        anchors.centerIn: parent
        width: 16
        height: 16
        source: root.iconPath
        visible: root.iconPath.length > 0
        mipmap: true
      }

      Text {
        anchors.fill: parent
        visible: root.iconPath.length === 0
        color: root.shell.mTertiary
        font.pixelSize: 12
        font.weight: Font.DemiBold
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        text: root.shell ? root.shell.appInitial(root.windowRow.appId) : ""
      }
    }

    Text {
      width: parent.width - 25
      height: parent.height
      color: root.windowFocused ? root.shell.mOnSurface : root.shell.mOnSurfaceVariant
      font.pixelSize: 12
      verticalAlignment: Text.AlignVCenter
      elide: Text.ElideRight
      text: root.shell ? root.shell.windowTitle(root.windowRow.id, root.shell.windowRevision) : ""
    }
  }

  HoverHandler {
    id: windowHover

    onHoveredChanged: {
      if (hovered) {
        previewTimer.restart();
      } else {
        previewTimer.stop();
        root.previewHidden();
      }
    }
  }

  Timer {
    id: previewTimer
    interval: 180
    repeat: false
    onTriggered: {
      if (!windowHover.hovered) {
        return;
      }
      const pos = root.mapToItem(null, root.width + 14, root.height / 2);
      root.previewRequested(root.windowRow, pos.x, pos.y, root.height);
    }
  }

  TapHandler {
    acceptedButtons: Qt.LeftButton
    gesturePolicy: TapHandler.ReleaseWithinBounds
    onTapped: root.selected(root.windowRow)
  }

  Component.onDestruction: root.previewHidden()
}
