import QtQuick

Rectangle {
  id: root

  property var shell
  readonly property var snapshot: shell.debugSnapshot || ({})
  readonly property var heartbeat: snapshot.heartbeat || ({})
  readonly property var paths: snapshot.paths || ({})
  readonly property var descriptions: shell.windowDescriptions || ({})
  readonly property var recentEvents: (shell.agentEvents || []).slice(-12).reverse()

  visible: !shell.sidebarCollapsed && shell.debugPaneOpen
  width: parent ? parent.width : shell.workspacePaneWidth
  height: parent ? parent.height : 0
  color: shell.mSurface

  function valueText(value) {
    if (value === undefined || value === null || value === "") {
      return "none";
    }
    return String(value);
  }

  function descriptionRows() {
    const rows = [];
    for (const key in descriptions) {
      const entry = descriptions[key] || {};
      rows.push({
        id: key,
        description: entry.description || "",
        updatedAt: entry.updatedAt || ""
      });
    }
    rows.sort((a, b) => Number(a.id) - Number(b.id));
    return rows;
  }

  Column {
    anchors.fill: parent
    anchors.margins: 10
    spacing: 10

    Row {
      width: parent.width
      height: 28
      spacing: 8

      Text {
        width: parent.width - closeButton.width - parent.spacing
        height: parent.height
        color: root.shell.mOnSurface
        font.pixelSize: 13
        font.weight: Font.DemiBold
        verticalAlignment: Text.AlignVCenter
        text: "tic debug"
      }

      Rectangle {
        id: closeButton
        width: 28
        height: 28
        radius: 8
        color: closeHover.hovered ? root.shell.capsuleHoverColor : root.shell.capsuleColor
        border.color: root.shell.mOutline

        Text {
          anchors.centerIn: parent
          color: root.shell.mOnSurfaceVariant
          font.pixelSize: 14
          text: "x"
        }

        HoverHandler {
          id: closeHover
        }

        TapHandler {
          acceptedButtons: Qt.LeftButton
          onTapped: root.shell.hideDebugPane()
        }
      }
    }

    Flickable {
      width: parent.width
      height: Math.max(0, parent.height - 38)
      contentWidth: width
      contentHeight: contentColumn.height
      clip: true

      Column {
        id: contentColumn
        width: parent.width
        spacing: 10

        Section {
          title: "daemon"
          rows: [
            { label: "agent", value: root.shell.agentStatus },
            { label: "snapshot", value: root.valueText(root.snapshot.time) },
            { label: "last event", value: root.valueText(root.heartbeat.lastEvent) }
          ]
        }

        Section {
          title: "heartbeat"
          rows: [
            { label: "L1 windows", value: root.valueText(root.heartbeat.windowsInitialized) + "/" + root.valueText(root.heartbeat.windowsTotal) },
            { label: "pending initial", value: root.valueText(root.heartbeat.windowsPendingInitial) },
            { label: "screenshots", value: root.valueText(root.heartbeat.bufferedScreenshots) },
            { label: "workspaces", value: root.valueText(root.heartbeat.workspacesTotal) },
            { label: "pending L2", value: root.valueText(root.heartbeat.pendingWorkspaceUpdates) },
            { label: "diff", value: root.heartbeat.enabled && root.heartbeat.enabled.screenshotDiff ? "on" : "off" }
          ]
        }

        Section {
          title: "paths"
          rows: [
            { label: "events", value: root.valueText(root.paths.events) },
            { label: "descriptions", value: root.valueText(root.paths.windowDescriptions) },
            { label: "history", value: root.valueText(root.paths.historyDir) }
          ]
        }

        Column {
          width: parent.width
          spacing: 6

          Text {
            width: parent.width
            color: root.shell.mOnSurface
            font.pixelSize: 12
            font.weight: Font.DemiBold
            text: "window descriptions"
          }

          Repeater {
            model: root.descriptionRows()

            Rectangle {
              required property var modelData

              width: parent.width
              height: 44
              radius: 8
              color: root.shell.capsuleColor
              border.color: root.shell.mOutline

              Column {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 2

                Text {
                  width: parent.width
                  color: root.shell.mTertiary
                  font.pixelSize: 10
                  elide: Text.ElideRight
                  text: "window " + modelData.id
                }

                Text {
                  width: parent.width
                  color: root.shell.mOnSurface
                  font.pixelSize: 12
                  elide: Text.ElideRight
                  text: modelData.description
                }
              }
            }
          }

          Text {
            visible: root.descriptionRows().length === 0
            width: parent.width
            color: root.shell.mOnSurfaceVariant
            font.pixelSize: 12
            text: "none"
          }
        }

        Column {
          width: parent.width
          spacing: 6

          Text {
            width: parent.width
            color: root.shell.mOnSurface
            font.pixelSize: 12
            font.weight: Font.DemiBold
            text: "recent stream"
          }

          Repeater {
            model: root.recentEvents

            Text {
              required property var modelData

              width: parent.width
              color: root.shell.mOnSurfaceVariant
              font.pixelSize: 11
              wrapMode: Text.Wrap
              text: "[" + (modelData.kind || "event") + "] " + (modelData.title || "") + ": " + (modelData.body || "")
            }
          }
        }
      }
    }
  }

  component Section: Column {
    property string title: ""
    property var rows: []

    width: parent ? parent.width : 0
    spacing: 6

    Text {
      width: parent.width
      color: root.shell.mOnSurface
      font.pixelSize: 12
      font.weight: Font.DemiBold
      text: parent.title
    }

    Repeater {
      model: parent.rows

      Row {
        required property var modelData

        width: parent.width
        height: 18
        spacing: 8

        Text {
          width: 94
          height: parent.height
          color: root.shell.mOnSurfaceVariant
          font.pixelSize: 11
          verticalAlignment: Text.AlignVCenter
          elide: Text.ElideRight
          text: modelData.label
        }

        Text {
          width: parent.width - 102
          height: parent.height
          color: root.shell.mOnSurface
          font.pixelSize: 11
          verticalAlignment: Text.AlignVCenter
          elide: Text.ElideRight
          text: modelData.value
        }
      }
    }
  }
}
