import QtQuick
import Quickshell
import qs.Commons
import qs.Modules.TicWorkspace
import qs.Services.UI
import qs.Widgets

NIconButton {
  id: root

  property ShellScreen screen
  property string widgetId: ""
  property string section: ""
  property int sectionWidgetIndex: -1
  property int sectionWidgetsCount: 0

  icon: "heartbeat"
  tooltipText: TicWorkspaceState.debugPaneOpen ? "Hide tic debug" : "Show tic debug"
  tooltipDirection: BarService.getTooltipDirection(screen?.name)
  baseSize: Style.getCapsuleHeightForScreen(screen?.name)
  applyUiScale: false
  customRadius: Style.radiusL
  colorBg: Style.capsuleColor
  colorFg: TicWorkspaceState.debugPaneOpen ? Color.mPrimary : Color.mOnSurfaceVariant
  colorBgHover: Color.mHover
  colorFgHover: Color.mOnHover
  colorBorder: TicWorkspaceState.debugPaneOpen ? Color.mPrimary : Style.capsuleBorderColor
  colorBorderHover: Style.capsuleBorderColor

  onClicked: TicWorkspaceState.toggleDebugPane()
}
