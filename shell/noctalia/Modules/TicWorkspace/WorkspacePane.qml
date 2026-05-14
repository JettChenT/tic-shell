import QtQuick

import "Widgets" as Widgets

Item {
  id: root

  property var shell

  width: shell.sidebarCollapsed ? shell.collapsedRailWidth : shell.workspacePaneWidth
  height: parent ? parent.height : 0

  Column {
    anchors.fill: parent
    anchors.margins: root.shell.sidebarCollapsed ? 5 : 8
    spacing: 6

    Row {
      id: headerRow

      width: parent.width
      height: 32
      spacing: root.shell.sidebarCollapsed ? 0 : 8

      Item {
        visible: !root.shell.sidebarCollapsed
        width: Math.max(0, parent.width - toggleAgentPaneButton.width - parent.spacing)
        height: parent.height
      }

      Widgets.SidebarButton {
        id: toggleAgentPaneButton

        visible: !root.shell.sidebarCollapsed
        label: "A"
        labelSize: 13
        labelWeight: Font.DemiBold
        labelColor: root.shell.agentPaneCollapsed ? root.shell.mOnSurfaceVariant : root.shell.mPrimary
        backgroundColor: root.shell.capsuleColor
        hoverColor: root.shell.capsuleHoverColor
        borderColor: root.shell.agentPaneCollapsed ? root.shell.mOutline : root.shell.mPrimary
        onClicked: root.shell.toggleAgentPane()
      }
    }

    Flickable {
      id: workspaceScroller

      visible: !root.shell.sidebarCollapsed
      width: parent.width
      height: Math.max(0, parent.height - headerRow.height - 8)
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
            onWindowPreviewRequested: (windowRow, x, y, height) => root.shell.showWindowPreview(windowRow, x, y, height)
            onWindowPreviewHidden: root.shell.hideWindowPreview()
          }
        }
      }
    }
  }
}
