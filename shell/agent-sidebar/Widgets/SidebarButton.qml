import QtQuick

Rectangle {
  id: root

  property string label: ""
  property color labelColor: "#8bd5ca"
  property int labelSize: 14
  property int labelWeight: Font.Normal
  property color backgroundColor: "#2b303b"
  property color hoverColor: "#3a4050"
  property color borderColor: "#596173"

  signal clicked

  width: 32
  height: 32
  radius: 6
  color: mouse.containsMouse ? hoverColor : backgroundColor
  border.color: borderColor

  Text {
    anchors.centerIn: parent
    color: root.labelColor
    font.pixelSize: root.labelSize
    font.weight: root.labelWeight
    text: root.label
  }

  MouseArea {
    id: mouse

    anchors.fill: parent
    hoverEnabled: true
    onClicked: root.clicked()
  }
}
