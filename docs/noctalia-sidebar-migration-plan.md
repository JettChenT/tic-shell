# Tic Shell Noctalia Left-Bar Integration Plan

## Product Goal

`tic-shell` should use Noctalia's existing vertical bar system instead of rebuilding Noctalia's system UI.

Noctalia already provides good launcher, clock, tray, notification, media, audio, brightness, battery, network, Bluetooth, settings, OSD, wallpaper, and session UI when the bar position is set to `left`. The `tic-shell` contribution is the expanded niri workspace and window rail, plus the attached Codex agent pane.

The target product is therefore:

```text
Noctalia left vertical bar
  top/system widgets from Noctalia
  expanded tic workspace/window rail in the workspace slot
  bottom/system widgets from Noctalia
  optional attached Codex pane from tic-shell
```

## Non-Negotiable Product Contracts

- Do not reimplement Noctalia panels, settings, OSD, wallpaper, tray, media, audio, brightness, battery, network, Bluetooth, or session UI in `tic-shell` unless a Noctalia surface cannot be adapted.
- Use Noctalia's `bar.position: "left"` behavior as the baseline shell chrome model.
- The `tic-shell` workspace sidebar is the expanded vertical-bar workspace widget, not a second independent system-control bar.
- Workspace and window navigation remain niri-first and owned by `tic-shell`.
- Noctalia `Workspace`, `ActiveWindow`, and `Taskbar` widgets should not duplicate the `tic-shell` workspace/window rail in the final integrated UI.
- `bin/tic-sidebar` remains the lifecycle and IPC surface for the workspace/agent rail during the transition.
- `just sidebar` continues to start the current workspace/agent rail from this checkout.

## Current Implementation Target

The current checkout vendors Noctalia under:

```text
shell/noctalia
```

`bin/tic-sidebar` now starts that in-repo Noctalia shell by default. The old focused workspace/agent rail remains available at `shell/agent-sidebar` as the source and fallback implementation, but the active migration target is the Noctalia-integrated bar.

The integrated Noctalia profile defaults to:

- `bar.position: "left"`
- top widgets: `Launcher`, `Clock`, `SystemMonitor`, `MediaMini`
- separate adjacent tic workspace section loaded beside the left bar
- bottom widgets: `Tray`, `NotificationHistory`, `Battery`, `Volume`, `Brightness`, `ControlCenter`
- no default Noctalia `Workspace`, `ActiveWindow`, or `Taskbar` widgets
- no upstream setup wizard or changelog prompt on first launch

The tic workspace adapter is installed at:

```text
shell/noctalia/Modules/MainScreen/TicWorkspaceWindow.qml
shell/noctalia/Modules/MainScreen/TicWorkspaceExclusionZone.qml
shell/noctalia/Modules/TicWorkspace/TicWorkspacePanel.qml
```

The focused rail dimensions are:

- compact rail width: `44`
- expanded workspace rail width: `250`
- optional attached agent pane width: `360`
- workspace and windows are listed vertically in the middle rail
- the Codex pane is independently collapsible
- Noctalia remains responsible for normal system widgets and panels through its own left-bar configuration

Noctalia's bar, panel, OSD, wallpaper, settings, and system services remain owned by the vendored Noctalia tree.

## Theme And UI Integration

The workspace rail should look like an extension of Noctalia, not a separate app embedded next to it.

The current rail follows Noctalia's Material-style theme token names:

- `mPrimary`
- `mOnPrimary`
- `mSecondary`
- `mTertiary`
- `mError`
- `mSurface`
- `mOnSurface`
- `mSurfaceVariant`
- `mOnSurfaceVariant`
- `mOutline`
- `mHover`
- `mOnHover`

`shell/agent-sidebar/Services/NoctaliaTheme.qml` reads Noctalia's generated color file:

```text
~/.config/noctalia/colors.json
```

If the file is missing, the rail falls back to Noctalia's default dark palette. Workspace cards, window rows, Codex controls, slash-command popups, and the rail background should consume these shared tokens instead of standalone `tic-shell` colors.

Geometry should also match Noctalia's bar language:

- rounded capsule-like cards and buttons
- surface-variant backgrounds
- primary-color focused workspace state
- hover states based on `mHover`
- outline borders based on `mOutline`

## Noctalia Settings Baseline

The relevant Noctalia setting is:

```json
{
  "bar": {
    "position": "left"
  }
}
```

Noctalia's default widget sections already map well to vertical layout:

```text
left section   -> top widgets
center section -> workspace slot
right section  -> bottom widgets
```

For the final integrated shell, the center `Workspace` widget slot should be replaced by the expanded `tic-shell` workspace/window rail. Noctalia system widgets should remain in the top and bottom sections.

## Completed Integration

The preferred end state is not two left bars. It is one Noctalia-style left bar where the center workspace slot is expanded and provided by `tic-shell`.

This checkout now implements that shape:

- `shell/noctalia/Commons/Settings.qml` and `shell/noctalia/Assets/settings-default.json` default Noctalia to a normal narrow left bar with no center workspace widget.
- `shell/noctalia/Modules/MainScreen/TicWorkspaceWindow.qml` loads the tic workspace/agent rail as a separate adjacent layer surface.
- `shell/noctalia/Modules/MainScreen/TicWorkspaceExclusionZone.qml` reserves workspace-rail width beside Noctalia's normal bar exclusion zone.
- `shell/noctalia/Modules/MainScreen/AllScreens.qml` creates the tic workspace surface only for vertical Noctalia bars.
- `bin/tic-sidebar` points at `shell/noctalia` and keeps the tic IPC verbs: `toggle`, `show`, `hide`, `toggle-agent`, `show-agent`, and `hide-agent`.

## Acceptance Criteria

This migration is complete when:

- Noctalia can run with `bar.position` set to `left` as the primary shell chrome.
- The center workspace slot is the expanded `tic-shell` workspace/window rail.
- Noctalia system widgets remain usable in the top and bottom vertical bar sections.
- The `tic-shell` workspace rail uses Noctalia theme colors and capsule-style geometry, including live color changes from Noctalia's generated `colors.json`.
- Noctalia workspace, active-window, and taskbar widgets are not duplicated alongside the `tic-shell` workspace rail.
- The Codex agent pane remains attached to the workspace rail and independently collapsible.
- `bin/tic-sidebar` can still start, stop, show, hide, and toggle the workspace/agent rail during the transition.

Current verification:

- fresh-profile Noctalia launch loads `shell/noctalia/shell.qml`
- generated settings contain `bar.position: "left"` and an empty Noctalia center widget list
- niri layers include separate `noctalia-bar-*` and `tic-workspace-*` surfaces
- duplicate-instance IPC succeeds for `sidebar toggleAgent` and `sidebar hideAgent`
