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

  icon: TicWorkspaceState.collapsed ? "chevron-right" : "chevron-left"
  tooltipText: TicWorkspaceState.collapsed ? "Show workspace rail" : "Hide workspace rail"
  tooltipDirection: BarService.getTooltipDirection(screen?.name)
  baseSize: Style.getCapsuleHeightForScreen(screen?.name)
  applyUiScale: false
  customRadius: Style.radiusL
  colorBg: Style.capsuleColor
  colorFg: TicWorkspaceState.collapsed ? Color.mOnSurfaceVariant : Color.mPrimary
  colorBgHover: Color.mHover
  colorFgHover: Color.mOnHover
  colorBorder: Style.capsuleBorderColor
  colorBorderHover: Style.capsuleBorderColor

  onClicked: TicWorkspaceState.toggleCollapsed()
}
