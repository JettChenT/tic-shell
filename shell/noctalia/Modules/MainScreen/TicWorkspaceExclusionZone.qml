import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Modules.TicWorkspace
import qs.Services.Compositor
import qs.Services.UI

PanelWindow {
  id: root

  readonly property string barPosition: Settings.getBarPositionForScreen(screen?.name)
  readonly property bool barIsVertical: barPosition === "left" || barPosition === "right"
  readonly property bool autoHide: Settings.getBarDisplayModeForScreen(screen?.name) === "auto_hide"
  readonly property bool nonExclusive: Settings.getBarDisplayModeForScreen(screen?.name) === "non_exclusive"
  readonly property bool barFloating: Settings.data.bar.barType === "floating"
  readonly property real barMarginH: Math.ceil(barFloating ? Settings.data.bar.marginHorizontal : 0)
  readonly property real ticWorkspaceWidth: TicWorkspaceState.reservedWidth()
  readonly property real bleedOffset: Settings.data.bar.enableExclusionZoneInset ? 1.0 : 0.0
  readonly property real bleedInset: {
    const info = CompositorService.displayScales[screen?.name];
    const scale = (info && info.scale) ? info.scale : 1.0;
    return bleedOffset / scale;
  }

  color: "transparent"
  visible: barIsVertical && ticWorkspaceWidth > 0
  mask: Region {}

  WlrLayershell.layer: WlrLayer.Top
  WlrLayershell.namespace: "tic-workspace-exclusion-" + barPosition + "-" + (screen?.name || "unknown")
  WlrLayershell.exclusionMode: ExclusionMode.Ignore

  anchors {
    top: true
    bottom: true
    left: barPosition === "left"
    right: barPosition === "right"
  }

  margins {
    left: barPosition === "left" ? Style.getBarHeightForScreen(screen?.name) + barMarginH : 0
    right: barPosition === "right" ? Style.getBarHeightForScreen(screen?.name) + barMarginH : 0
  }

  implicitWidth: ticWorkspaceWidth - bleedInset
  implicitHeight: 0
}
