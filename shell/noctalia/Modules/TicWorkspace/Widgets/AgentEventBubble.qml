import QtQuick

Rectangle {
  id: root

  property var shell
  property var event
  readonly property bool isUser: event.kind === "user"
  readonly property bool isAssistant: event.kind === "assistant"
  readonly property bool isThought: event.kind === "thought"
  readonly property bool isThinking: event.kind === "thinking"
  readonly property bool isTool: event.kind === "tool"
  readonly property bool isPermission: event.kind === "permission"
  readonly property bool hasHeader: isTool || isPermission || isThinking || isThought

  width: parent ? parent.width : 0
  height: Math.max(isThinking ? 38 : 46, eventBody.implicitHeight + (hasHeader ? 36 : 20))
  radius: 16
  color: isUser ? root.shell.capsuleColor : (isThinking ? Qt.alpha(root.shell.mSurfaceVariant, 0.72) : Qt.alpha(root.shell.mSurface, 0.5))
  border.color: isTool ? root.shell.mSecondary : (isPermission ? root.shell.mTertiary : (isThinking ? root.shell.mOutline : "transparent"))
  border.width: hasHeader || isUser ? 1 : 0

  Column {
    anchors.fill: parent
    anchors.margins: 9
    spacing: 5

    Row {
      visible: root.hasHeader
      width: parent.width
      height: 15
      spacing: 6

      Text {
        width: parent.width - eventTime.width - parent.spacing
        height: parent.height
        color: root.isTool ? root.shell.mSecondary : (root.isPermission ? root.shell.mTertiary : (root.isThinking ? root.shell.mPrimary : root.shell.mTertiary))
        font.pixelSize: 12
        font.weight: Font.DemiBold
        text: root.event.title
        elide: Text.ElideRight
      }

      Text {
        id: eventTime

        visible: !root.isThinking
        width: 38
        height: parent.height
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 11
        horizontalAlignment: Text.AlignRight
        text: root.event.time
      }
    }

    Text {
      id: eventBody

      visible: root.event.kind !== "thinking" || root.event.body.length > 0
      width: parent.width
      color: root.shell.mOnSurface
      font.pixelSize: 12
      wrapMode: Text.Wrap
      text: root.event.body
    }
  }
}
