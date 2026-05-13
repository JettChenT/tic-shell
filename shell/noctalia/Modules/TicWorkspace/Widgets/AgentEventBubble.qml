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
  readonly property string bodyText: String(event.body || "")
  readonly property var bodyLines: bodyText.split("\n")
  readonly property string toolStatus: isTool && bodyLines.length > 0 ? bodyLines[0] : ""
  readonly property string toolOutput: isTool && bodyLines.length > 1 ? bodyLines.slice(1).join("\n").trim() : ""
  readonly property int toolOutputLines: toolOutput.length > 0 ? toolOutput.split("\n").length : 0
  readonly property bool hasToolOutput: isTool && toolOutput.length > 0
  property bool toolExpanded: false

  function previewText() {
    if (!hasToolOutput) {
      return toolStatus;
    }
    const lines = toolOutput.split("\n");
    const preview = lines.slice(0, 3).join("\n").trim();
    if (lines.length > 3) {
      return preview + "\n...";
    }
    return preview;
  }

  width: parent ? parent.width : 0
  height: root.isTool
    ? Math.max(52, toolContent.y + toolContent.implicitHeight + 14)
    : Math.max(isThinking ? 38 : 46, eventBody.implicitHeight + (hasHeader ? 36 : 20))
  radius: root.isTool ? 8 : 16
  color: isUser ? root.shell.capsuleColor : (isThinking ? Qt.alpha(root.shell.mSurfaceVariant, 0.72) : (isTool ? Qt.alpha(root.shell.mSurface, 0.62) : Qt.alpha(root.shell.mSurface, 0.5)))
  border.color: isTool ? Qt.alpha(root.shell.mOutline, 0.72) : (isPermission ? root.shell.mTertiary : (isThinking ? root.shell.mOutline : "transparent"))
  border.width: hasHeader || isUser ? 1 : 0

  Column {
    anchors.fill: parent
    anchors.margins: root.isTool ? 10 : 9
    spacing: root.isTool ? 7 : 5

    Row {
      id: eventHeader

      visible: root.hasHeader
      width: parent.width
      height: root.isTool ? 22 : 15
      spacing: 6

      Text {
        width: parent.width - eventTime.width - (toolStatusLabel.visible ? toolStatusLabel.width : 0) - (toolDisclosure.visible ? toolDisclosure.width : 0) - parent.spacing * (toolDisclosure.visible ? 3 : 1)
        height: parent.height
        color: root.isTool ? root.shell.mPrimary : (root.isPermission ? root.shell.mTertiary : (root.isThinking ? root.shell.mPrimary : root.shell.mTertiary))
        font.pixelSize: 12
        font.weight: Font.DemiBold
        text: root.event.title
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
      }

      Text {
        id: toolStatusLabel

        visible: root.isTool && root.toolStatus.length > 0
        width: Math.min(76, implicitWidth)
        height: parent.height
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 10
        text: root.toolStatus
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
      }

      Text {
        id: toolDisclosure

        visible: root.isTool && root.hasToolOutput
        width: 16
        height: parent.height
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 14
        font.weight: Font.DemiBold
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        text: root.toolExpanded ? "-" : "+"
      }

      Text {
        id: eventTime

        visible: !root.isThinking
        width: 38
        height: parent.height
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 11
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
        text: root.event.time
      }

      TapHandler {
        enabled: root.isTool && root.hasToolOutput
        acceptedButtons: Qt.LeftButton
        onTapped: root.toolExpanded = !root.toolExpanded
      }
    }

    Column {
      id: toolContent

      visible: root.isTool
      width: parent.width
      height: implicitHeight
      spacing: 5

      Rectangle {
        visible: root.hasToolOutput
        width: parent.width
        height: root.toolExpanded ? Math.min(226, toolOutputBody.implicitHeight + 24) : Math.min(74, toolPreviewBody.implicitHeight + 24)
        radius: 6
        color: Qt.alpha(root.shell.mSurfaceVariant, 0.38)
        border.color: Qt.alpha(root.shell.mOutline, 0.42)
        border.width: 1
        clip: true

        Text {
          id: toolPreviewBody

          visible: !root.toolExpanded
          anchors.fill: parent
          anchors.margins: 9
          color: root.shell.mOnSurface
          font.family: "monospace"
          font.pixelSize: 11
          wrapMode: Text.Wrap
          text: root.previewText()
        }

        Flickable {
          visible: root.toolExpanded
          anchors.fill: parent
          anchors.margins: 9
          clip: true
          contentWidth: width
          contentHeight: toolOutputBody.implicitHeight

          Text {
            id: toolOutputBody

            width: parent.width
            color: root.shell.mOnSurface
            font.family: "monospace"
            font.pixelSize: 11
            wrapMode: Text.Wrap
            text: root.toolOutput
          }
        }
      }

      Text {
        visible: root.hasToolOutput && !root.toolExpanded && root.toolOutputLines > 3
        width: parent.width - 2
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 10
        text: root.toolOutputLines + " output lines"
        horizontalAlignment: Text.AlignRight
      }

      Item {
        visible: root.hasToolOutput
        width: parent.width
        height: 3
      }
    }

    Text {
      id: eventBody

      visible: !root.isTool && (root.event.kind !== "thinking" || root.bodyText.length > 0)
      width: parent.width
      color: root.shell.mOnSurface
      font.pixelSize: 12
      wrapMode: Text.Wrap
      text: root.bodyText
    }
  }
}
