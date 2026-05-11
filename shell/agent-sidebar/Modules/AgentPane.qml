import QtQuick

import "../Widgets" as Widgets

Item {
  id: root

  property var shell

  visible: !shell.sidebarCollapsed && !shell.agentPaneCollapsed
  width: shell.agentPaneWidth
  height: parent ? parent.height : 0

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

        Widgets.SidebarButton {
          id: newSessionButton

          width: 28
          height: 28
          label: "+"
          labelSize: 18
          onClicked: root.shell.sendAgentControl("new")
        }

        Widgets.SidebarButton {
          id: clearSessionButton

          width: 28
          height: 28
          label: "C"
          labelColor: "#cad3f5"
          labelSize: 13
          labelWeight: Font.DemiBold
          onClicked: root.shell.sendAgentControl("clear")
        }

        Widgets.SidebarButton {
          id: cancelSessionButton

          width: 28
          height: 28
          label: "x"
          labelColor: "#ed8796"
          labelSize: 15
          labelWeight: Font.DemiBold
          onClicked: root.shell.sendAgentControl("cancel")
        }
      }

      Text {
        visible: root.shell.agentStatus === "error" || root.shell.agentStatus === "stopped"
        width: parent.width
        height: 16
        color: "#ed8796"
        font.pixelSize: 12
        text: visible ? root.shell.agentStatus : ""
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
          model: root.shell.agentEvents

          Widgets.AgentEventBubble {
            event: modelData
          }
        }
      }
    }

    Widgets.AgentPromptBox {
      id: agentInputBox

      shell: root.shell
      onPromptAccepted: text => root.shell.sendAgentPrompt(text)
    }
  }
}
