import QtQuick
import qs.Services.UI

Rectangle {
  id: root

  property string label: ""
  property string tooltipText: ""
  property string tooltipDirection: "right"
  property color labelColor: "#fff59b"
  property int labelSize: 14
  property int labelWeight: Font.Normal
  property color backgroundColor: "#11112d"
  property color hoverColor: Qt.alpha("#9BFECE", 0.25)
  property color borderColor: "#21215F"

  signal clicked

  width: 32
  height: 32
  radius: 12
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
    onEntered: {
      if (root.tooltipText.length > 0) {
        TooltipService.show(root, root.tooltipText, root.tooltipDirection);
      }
    }
    onExited: TooltipService.hide()
  }
}
