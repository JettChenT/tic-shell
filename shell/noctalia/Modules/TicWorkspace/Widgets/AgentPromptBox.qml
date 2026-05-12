import QtQuick

import "." as Widgets

Rectangle {
  id: root

  property var shell
  readonly property var commands: shell.filteredAgentCommands(agentPromptInput.text)

  signal promptAccepted(string text)

  function completeCommand(command) {
    if (!command) {
      return;
    }
    agentPromptInput.text = "/" + command.name + " ";
    agentPromptInput.cursorPosition = agentPromptInput.text.length;
    agentPromptInput.forceActiveFocus();
  }

  width: parent ? parent.width : 0
  height: 76
  z: 10
  radius: 16
  color: root.shell.capsuleColor
  border.color: agentPromptInput.activeFocus ? root.shell.mPrimary : root.shell.mOutline

  function clearPrompt() {
    agentPromptInput.text = "";
  }

  Widgets.SlashCommandPopup {
    id: slashCommandPopup

    shell: root.shell
    commands: root.commands
    currentIndex: root.shell.slashCommandIndex
    visible: agentPromptInput.activeFocus && commands.length > 0
    onCommandHovered: index => root.shell.slashCommandIndex = index
    onCommandClicked: command => root.completeCommand(command)
  }

  Column {
    anchors.fill: parent
    anchors.margins: 9
    spacing: 7

    TextInput {
      id: agentPromptInput

      width: parent.width
      height: 26
      color: root.shell.mOnSurface
      selectedTextColor: root.shell.mOnPrimary
      selectionColor: root.shell.mPrimary
      font.pixelSize: 13
      clip: true
      selectByMouse: true
      verticalAlignment: TextInput.AlignVCenter

      onTextChanged: {
        if (!text.startsWith("/")) {
          root.shell.slashCommandIndex = 0;
        } else if (root.shell.slashCommandIndex >= root.commands.length) {
          root.shell.slashCommandIndex = Math.max(0, root.commands.length - 1);
        }
      }
      onAccepted: {
        if (slashCommandPopup.visible && root.shell.selectedSlashCommand(text) !== null && text.indexOf(" ") === -1) {
          root.completeCommand(root.shell.selectedSlashCommand(text));
        } else {
          root.promptAccepted(text);
          text = "";
        }
      }

      Keys.onDownPressed: function(event) {
        if (root.commands.length > 0) {
          root.shell.slashCommandIndex = Math.min(root.commands.length - 1, root.shell.slashCommandIndex + 1);
          slashCommandPopup.positionCurrent();
          event.accepted = true;
        }
      }

      Keys.onUpPressed: function(event) {
        if (root.commands.length > 0) {
          root.shell.slashCommandIndex = Math.max(0, root.shell.slashCommandIndex - 1);
          slashCommandPopup.positionCurrent();
          event.accepted = true;
        }
      }

      Keys.onEscapePressed: function(event) {
        if (slashCommandPopup.visible) {
          text = "";
          root.shell.slashCommandIndex = 0;
          event.accepted = true;
        }
      }

      Keys.onTabPressed: function(event) {
        if (slashCommandPopup.visible && root.shell.selectedSlashCommand(text) !== null) {
          root.completeCommand(root.shell.selectedSlashCommand(text));
          event.accepted = true;
        }
      }

      Text {
        anchors.fill: parent
        visible: agentPromptInput.text.length === 0 && !agentPromptInput.activeFocus
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 13
        verticalAlignment: Text.AlignVCenter
        text: "ask Codex"
        elide: Text.ElideRight
      }
    }

    Row {
      width: parent.width
      height: 24
      spacing: 8

      Text {
        width: parent.width - sendPromptButton.width - parent.spacing
        height: parent.height
        color: root.shell.mOnSurfaceVariant
        font.pixelSize: 11
        verticalAlignment: Text.AlignVCenter
        text: "All actions allowed"
        elide: Text.ElideRight
      }

      Widgets.SidebarButton {
        id: sendPromptButton

        width: 64
        height: 24
        label: "Send"
        labelColor: root.shell.mOnSurface
        labelSize: 12
        labelWeight: Font.DemiBold
        backgroundColor: root.shell.capsuleColor
        hoverColor: root.shell.capsuleHoverColor
        borderColor: root.shell.mPrimary
        onClicked: {
          root.promptAccepted(agentPromptInput.text);
          agentPromptInput.text = "";
        }
      }
    }
  }
}
