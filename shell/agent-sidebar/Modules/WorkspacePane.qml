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

      Item {
        visible: !root.shell.sidebarCollapsed
        width: parent.width - collapseSidebarButton.width - toggleAgentPaneButton.width - parent.spacing
        height: parent.height
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
