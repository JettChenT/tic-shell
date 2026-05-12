import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Modules.TicWorkspace
import qs.Services.UI

PanelWindow {
  id: root

  color: "transparent"
  visible: BarService.effectivelyVisible && barIsVertical && !TicWorkspaceState.collapsed

  WlrLayershell.namespace: "tic-workspace-" + (screen?.name || "unknown")
  WlrLayershell.layer: WlrLayer.Top
  WlrLayershell.exclusionMode: ExclusionMode.Ignore
  WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

  readonly property string barPosition: Settings.getBarPositionForScreen(screen?.name)
  readonly property bool barIsVertical: barPosition === "left" || barPosition === "right"
  readonly property real barHeight: Style.getBarHeightForScreen(screen?.name)
  readonly property bool barFloating: Settings.data.bar.barType === "floating"
  readonly property real barMarginH: Math.ceil(barFloating ? Settings.data.bar.marginHorizontal : 0)
  readonly property real barOffset: barHeight + barMarginH

  anchors {
    top: true
    bottom: true
    left: barPosition === "left"
    right: barPosition === "right"
  }

  margins {
    left: barPosition === "left" ? barOffset : 0
    right: barPosition === "right" ? barOffset : 0
  }

  implicitWidth: TicWorkspaceState.reservedWidth()
  implicitHeight: screen ? screen.height : 1

  Rectangle {
    anchors.fill: parent
    color: Qt.alpha(Color.mSurface, Settings.data.ui.panelBackgroundOpacity ?? 0.93)
    border.color: Color.mOutline
    border.width: 1
  }

  TicWorkspacePanel {
    id: workspace

    screen: root.screen
    width: TicWorkspaceState.reservedWidth()
    anchors {
      top: parent.top
      bottom: parent.bottom
      left: parent.left
    }
  }
}
