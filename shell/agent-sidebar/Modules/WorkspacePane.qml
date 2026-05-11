import QtQuick

import "../Widgets" as Widgets

Item {
  id: root

  property var shell

  width: shell.sidebarCollapsed ? shell.collapsedRailWidth : shell.workspacePaneWidth
  height: parent ? parent.height : 0

  Column {
    anchors.fill: parent
    anchors.margins: root.shell.sidebarCollapsed ? 6 : 12
    spacing: 10

    Row {
      width: parent.width
      height: 32
      spacing: root.shell.sidebarCollapsed ? 0 : 8

      Widgets.SidebarButton {
        id: collapseSidebarButton

        label: root.shell.sidebarCollapsed ? ">" : "<"
        labelSize: 18
        labelWeight: Font.DemiBold
        onClicked: root.shell.toggleSidebar()
      }

      Text {
        visible: !root.shell.sidebarCollapsed
        width: parent.width - collapseSidebarButton.width - addWorkspaceButton.width - toggleAgentPaneButton.width - parent.spacing * 3
        height: parent.height
        color: "#cad3f5"
        font.pixelSize: 17
        font.weight: Font.DemiBold
        verticalAlignment: Text.AlignVCenter
        text: "Workspaces"
        elide: Text.ElideRight
      }

      Widgets.SidebarButton {
        id: addWorkspaceButton

        visible: !root.shell.sidebarCollapsed
        label: "+"
        labelSize: 22
        onClicked: root.shell.focusBottomWorkspace()
      }

      Widgets.SidebarButton {
        id: toggleAgentPaneButton

        visible: !root.shell.sidebarCollapsed
        label: "C"
        labelSize: 13
        labelWeight: Font.DemiBold
        labelColor: root.shell.agentPaneCollapsed ? "#7f8797" : "#8bd5ca"
        borderColor: root.shell.agentPaneCollapsed ? "#596173" : "#8bd5ca"
        onClicked: root.shell.toggleAgentPane()
      }
    }

    Flickable {
      id: workspaceScroller

      visible: !root.shell.sidebarCollapsed
      width: parent.width
      height: parent.height - y
      clip: true
      contentWidth: width
      contentHeight: workspaceColumn.height

      Column {
        id: workspaceColumn

        width: workspaceScroller.width
        spacing: 8

        Repeater {
          model: root.shell.workspaceRows

          Widgets.WorkspaceCard {
            shell: root.shell
            workspace: modelData
            onSelected: workspace => root.shell.focusWorkspace(workspace)
            onAnnotationAccepted: (workspaceId, annotation) => root.shell.setAnnotation(workspaceId, annotation)
            onWindowSelected: windowRow => root.shell.focusWindow(windowRow)
          }
        }
      }
    }
  }
}
