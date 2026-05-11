import QtQuick
import QtQuick.Controls

Rectangle {
  id: root

  property var shell
  property var commands: []
  property int currentIndex: 0

  signal commandHovered(int index)
  signal commandClicked(var command)

  width: parent ? parent.width : 0
  height: visible ? Math.min(166, commands.length * 38 + 10) : 0
  x: 0
  y: -height - 6
  z: 20
  radius: 7
  color: "#252a34"
  border.color: "#596173"
  clip: true

  function positionCurrent() {
    slashCommandList.positionViewAtIndex(root.currentIndex, ListView.Contain);
  }

  onCommandsChanged: {
    if (root.shell && root.shell.slashCommandIndex >= commands.length) {
      root.shell.slashCommandIndex = Math.max(0, commands.length - 1);
    }
    positionCurrent();
  }

  ListView {
    id: slashCommandList

    anchors.fill: parent
    anchors.margins: 5
    clip: true
    spacing: 3
    model: root.commands
    currentIndex: root.currentIndex
    boundsBehavior: Flickable.StopAtBounds

    onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

    delegate: Rectangle {
      readonly property bool selected: index === root.currentIndex

      width: slashCommandList.width
      height: 35
      radius: 5
      color: selected || slashCommandMouse.containsMouse ? "#303642" : "transparent"

      Row {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        spacing: 8

        Text {
          width: 86
          height: parent.height
          color: "#8bd5ca"
          font.pixelSize: 12
          font.weight: Font.DemiBold
          verticalAlignment: Text.AlignVCenter
          text: "/" + modelData.name
          elide: Text.ElideRight
        }

        Text {
          width: parent.width - 94
          height: parent.height
          color: "#b8c0d6"
          font.pixelSize: 11
          verticalAlignment: Text.AlignVCenter
          text: modelData.description
          elide: Text.ElideRight
        }
      }

      MouseArea {
        id: slashCommandMouse

        anchors.fill: parent
        hoverEnabled: true
        onEntered: root.commandHovered(index)
        onClicked: root.commandClicked(modelData)
      }
    }
  }

  Rectangle {
    visible: slashCommandList.contentHeight > slashCommandList.height
    width: 3
    height: Math.max(18, slashCommandList.height * slashCommandList.height / slashCommandList.contentHeight)
    x: parent.width - width - 3
    y: 5 + (slashCommandList.height - height) * (slashCommandList.contentY / Math.max(1, slashCommandList.contentHeight - slashCommandList.height))
    radius: 2
    color: "#7f8797"
    opacity: 0.75
  }
}
