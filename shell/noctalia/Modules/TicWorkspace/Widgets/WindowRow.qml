import QtQuick
import Quickshell.Widgets

Rectangle {
  id: root

  property var shell
  property var windowRow
  readonly property string iconPath: shell ? shell.iconForAppId(windowRow.appId) : ""
  readonly property bool windowFocused: shell ? shell.windowFocused(windowRow.id, shell.windowRevision) : false
  readonly property string windowTitle: shell ? shell.windowTitle(windowRow.id, shell.windowRevision) : ""
  readonly property string windowDescription: shell ? shell.windowDescription(windowRow.id, shell.windowRevision) : ""
  readonly property bool hasDescription: windowDescription.length > 0
  readonly property bool oneLineDescription: !hasDescription || !shell || shell.windowDescriptionsOneLine

  signal selected(var windowRow)
  signal previewRequested(var windowRow, real x, real y, real height)
  signal previewHidden()

  width: parent ? parent.width : 0
  height: oneLineDescription ? 28 : 44
  radius: 12
  color: windowFocused ? Qt.alpha(root.shell.mHover, 0.28) : (windowHover.hovered ? Qt.alpha(root.shell.mHover, 0.18) : Qt.alpha(root.shell.mSurface, 0.35))
  border.color: windowFocused ? root.shell.mHover : root.shell.mOutline

  Row {
    visible: root.oneLineDescription
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

    Row {
      width: parent.width - 25
      height: parent.height
      spacing: 5

      Text {
        id: titleText

        width: root.hasDescription ? Math.min(implicitWidth, parent.width * 0.62) : parent.width
        height: parent.height
        color: root.windowFocused ? root.shell.mOnSurface : root.shell.mOnSurfaceVariant
        font.pixelSize: 12
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        text: root.windowTitle
      }

      Text {
        id: separatorText

        visible: root.hasDescription
        width: visible ? 4 : 0
        height: parent.height
        color: root.shell.mTertiary
        font.pixelSize: 12
        verticalAlignment: Text.AlignVCenter
        text: "·"
      }

      Text {
        visible: root.hasDescription
        width: visible ? Math.max(0, parent.width - parent.spacing * 2 - titleText.width - separatorText.width) : 0
        height: parent.height
        color: root.shell.mTertiary
        font.pixelSize: 12
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        text: root.windowDescription
      }
    }
  }

  Item {
    visible: !root.oneLineDescription
    anchors.fill: parent

    Row {
      x: 8
      y: 0
      width: parent.width - 16
      height: 24
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
        text: root.windowTitle
      }
    }

    Text {
      x: 8
      y: 24
      width: parent.width - 16
      height: 18
      color: root.shell.mTertiary
      font.pixelSize: 11
      verticalAlignment: Text.AlignVCenter
      elide: Text.ElideRight
      text: root.windowDescription
    }
  }

  Rectangle {
    visible: windowHover.hovered && root.hasDescription
    z: 10
    x: 28
    y: root.height + 2
    width: Math.min(implicitDescription.implicitWidth + 14, root.width - x - 4)
    height: 22
    radius: 6
    color: root.shell.mSurface
    border.color: root.shell.mOutline

    Text {
      id: implicitDescription

      anchors.fill: parent
      anchors.leftMargin: 7
      anchors.rightMargin: 7
      color: root.shell.mOnSurfaceVariant
      font.pixelSize: 11
      verticalAlignment: Text.AlignVCenter
      elide: Text.ElideRight
      text: root.windowDescription
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
