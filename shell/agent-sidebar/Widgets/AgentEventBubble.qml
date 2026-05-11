import QtQuick

Rectangle {
  id: root

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
  radius: 7
  color: isTool ? "#26333b" : (isPermission ? "#332f3c" : (isUser ? "#303642" : (isThinking ? "#222832" : "#20242c")))
  border.color: isTool ? "#8aadf4" : (isPermission ? "#c6a0f6" : (isThinking ? "#596173" : "transparent"))
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
        color: root.isTool ? "#8aadf4" : (root.isPermission ? "#c6a0f6" : (root.isThinking ? "#eed49f" : "#a6da95"))
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
        color: "#7f8797"
        font.pixelSize: 11
        horizontalAlignment: Text.AlignRight
        text: root.event.time
      }
    }

    Text {
      id: eventBody

      visible: root.event.kind !== "thinking" || root.event.body.length > 0
      width: parent.width
      color: "#b8c0d6"
      font.pixelSize: 12
      wrapMode: Text.Wrap
      text: root.event.body
    }
  }
}
