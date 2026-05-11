import QtQuick
import Quickshell.Widgets

Rectangle {
  id: root

  property var shell
  property var windowRow
  readonly property string iconPath: shell ? shell.iconForAppId(windowRow.appId) : ""
  readonly property bool windowFocused: shell ? shell.windowFocused(windowRow.id, shell.windowRevision) : false

  signal selected(var windowRow)

  width: parent ? parent.width : 0
  height: 28
  radius: 5
  color: windowFocused ? "#3d4b4f" : (windowHover.hovered ? "#303642" : "#272d37")
  border.color: windowFocused ? "#8bd5ca" : "#3a4050"

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
        color: "#a6da95"
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
      color: root.windowFocused ? "#ffffff" : "#b8c0d6"
      font.pixelSize: 12
      verticalAlignment: Text.AlignVCenter
      elide: Text.ElideRight
      text: root.shell ? root.shell.windowTitle(root.windowRow.id, root.shell.windowRevision) : ""
    }
  }

  HoverHandler {
    id: windowHover
  }

  TapHandler {
    acceptedButtons: Qt.LeftButton
    gesturePolicy: TapHandler.ReleaseWithinBounds
    onTapped: root.selected(root.windowRow)
  }
}
