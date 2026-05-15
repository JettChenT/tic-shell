import QtQuick

import "Widgets" as Widgets

Item {
  id: root

  property var shell

  width: shell.sidebarCollapsed ? shell.collapsedRailWidth : shell.workspacePaneWidth
  height: parent ? parent.height : 0

  function colorForCursorTheme(theme) {
    const name = String(theme || "").toLowerCase();
    if (name.indexOf("green") !== -1) {
      return "#39d98a";
    }
    if (name.indexOf("orange") !== -1) {
      return "#ff9f43";
    }
    if (name.indexOf("pink") !== -1) {
      return "#ff6fb1";
    }
    if (name.indexOf("purple") !== -1) {
      return "#a78bfa";
    }
    if (name.indexOf("yellow") !== -1) {
      return "#facc15";
    }
    return "#4cc9f0";
  }

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
        width: Math.max(0, parent.width - forkSessionRow.width - toggleAgentPaneButton.width - parent.spacing * 2)
        height: parent.height
      }

      Row {
        id: forkSessionRow

        visible: !root.shell.sidebarCollapsed && (root.shell.forkSessions || []).length > 0
        height: parent.height
        width: Math.min(childrenRect.width, Math.max(0, parent.width - toggleAgentPaneButton.width - parent.spacing))
        spacing: 4
        layoutDirection: Qt.RightToLeft
        clip: true

        Repeater {
          model: (root.shell.forkSessions || []).slice().reverse()

          Widgets.SidebarButton {
            property var forkSession: modelData

            label: forkSession.status === "error" ? "!" : "C"
            labelSize: 12
            labelWeight: Font.DemiBold
            labelColor: forkSession.status === "done" ? root.shell.mOnSurfaceVariant : root.shell.mOnPrimary
            backgroundColor: forkSession.status === "error" ? root.shell.mError : root.colorForCursorTheme(forkSession.cursorTheme)
            hoverColor: root.shell.capsuleHoverColor
            borderColor: forkSession.selected ? root.shell.mPrimary : root.shell.mOutline
            onClicked: root.shell.selectForkSession(forkSession)
          }
        }
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
