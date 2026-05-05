# Niri Phase 0 Ground Truth

Date: 2026-05-06

Source plan: `/home/jettc/dev/lnx/docs/left-sidebar-acp-implementation-plan.md`

This file records the Phase 0 sidebar reservation test and the niri IPC shapes observed on the current machine. The plan source in `/home/jettc/dev/lnx` was used as read-only input; this note lives in `tic-shell`.

## Environment

- niri version: `niri 26.04 (Nixpkgs)`
- live socket used for checks: `/run/user/1000/niri.wayland-1.54475.sock`
- output: `Virtual-1`
- logical output size: `1512x982`
- output scale: `2.0`
- persisted niri config source: `/etc/nixos/niri/config.kdl`
- active Home Manager config after switch: `/home/jettc/.config/niri/config.kdl`

## Sidebar Reservation

Initial niri strut tested in `/etc/nixos/niri/config.kdl`:

```kdl
layout {
    struts {
        left 250
    }

    gaps 16
    center-focused-column "never"
    default-column-width { proportion 0.5; }
}
```

Validation and apply steps run during the strut test:

```sh
niri validate -c /etc/nixos/niri/config.kdl
nixos-rebuild switch --flake /etc/nixos#nixos
env NIRI_SOCKET=/run/user/1000/niri.wayland-1.54475.sock niri msg action load-config-file
```

Results:

- Config validation passed.
- NixOS/Home Manager switch completed.
- Active niri config symlink was repointed to a new Home Manager store file containing `left 250`.
- After `load-config-file`, focused Ghostty tiled geometry changed from the pre-strut `1027x920` to `880x920` with the initial 600px strut, then to `1130x920` with 350px, then to `1230x920` with the final 250px strut. This confirmed the reserved left rail changes available tiled window space.
- Failed behavior: after focusing a right-side column, niri's scrolling layout can still pan columns leftward under the strut area. This means niri struts are not a hard persistent reservation for a permanent left sidebar.
- The strut was removed from `/etc/nixos/niri/config.kdl` after this failure was confirmed.

Niri's bundled default config describes struts as outer gaps and notes that left/right struts keep the next side window visible. That matches the observed behavior: struts affect tile sizing, but they do not guarantee that the scrolling column array never appears in the left rail.

## Layer-Shell Exclusive Zone Test

A temporary Quickshell layer-shell panel was tested with `ExclusionMode.Auto`:

```qml
import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
  PanelWindow {
    color: "#20242c"
    implicitWidth: 250

    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "tic-shell-left-reserve-test"
    WlrLayershell.exclusionMode: ExclusionMode.Auto
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    anchors {
      top: true
      bottom: true
      left: true
    }
  }
}
```

Run command:

```sh
sudo -u jettc env \
  XDG_RUNTIME_DIR=/run/user/1000 \
  WAYLAND_DISPLAY=wayland-1 \
  NIRI_SOCKET=/run/user/1000/niri.wayland-1.54475.sock \
  /nix/store/iyi8sawnx7w04rq2sva9mxl7bm1xfkjd-quickshell-2026-05-03_d3e26cc/bin/qs \
  -p /tmp/tic-shell-left-reserve-test \
  --allow-duplicate
```

Results:

- `niri msg --json layers` showed `tic-shell-left-reserve-test` on the `Top` layer.
- With only the layer-shell panel active and no niri strut, focused Ghostty tile geometry was `1230x920`.
- A screenshot after `focus-column-right` showed the left panel remained visible and unobscured.
- niri can still scroll older columns behind the top-layer panel, so the real sidebar must be opaque and should use layer-shell exclusive-zone reservation as the source of truth.
- Do not combine niri left struts with sidebar layer-shell exclusive zones, because that double-reserves horizontal space.
- Noctalia layer surfaces continued to render according to `niri msg --json layers`.

The repo now includes the reusable Phase 0 shell at:

```text
shell/agent-sidebar/shell.qml
```

Run it with:

```sh
sudo -u jettc env \
  XDG_RUNTIME_DIR=/run/user/1000 \
  WAYLAND_DISPLAY=wayland-1 \
  NIRI_SOCKET=/run/user/1000/niri.wayland-1.54475.sock \
  /nix/store/iyi8sawnx7w04rq2sva9mxl7bm1xfkjd-quickshell-2026-05-03_d3e26cc/bin/qs \
  -p /home/jettc/osdev/tic-shell/shell/agent-sidebar \
  --allow-duplicate
```

## IPC Commands

Use the explicit socket when the shell was not launched inside niri:

```sh
export NIRI_SOCKET=/run/user/1000/niri.wayland-1.54475.sock
niri msg --json outputs
niri msg --json workspaces
niri msg --json windows
niri msg --json focused-window
niri msg --json layers
timeout 2s niri msg --json event-stream
```

## JSON Shapes

### outputs

Top-level shape is an object keyed by output name:

```json
{
  "Virtual-1": {
    "name": "Virtual-1",
    "make": "Red Hat, Inc.",
    "model": "QEMU Monitor",
    "serial": null,
    "physical_size": [760, 480],
    "modes": [
      {
        "width": 3024,
        "height": 1964,
        "refresh_rate": 59948,
        "is_preferred": false
      }
    ],
    "current_mode": 0,
    "is_custom_mode": true,
    "vrr_supported": false,
    "vrr_enabled": false,
    "logical": {
      "x": 0,
      "y": 0,
      "width": 1512,
      "height": 982,
      "scale": 2.0,
      "transform": "Normal"
    }
  }
}
```

### workspaces

Shape is an array of workspace objects:

```json
[
  {
    "id": 1,
    "idx": 1,
    "name": null,
    "output": "Virtual-1",
    "is_urgent": false,
    "is_active": true,
    "is_focused": true,
    "active_window_id": 2
  }
]
```

Observed fields:

- `id`: stable numeric niri workspace identity for the current session.
- `idx`: visible workspace index.
- `name`: optional niri workspace name.
- `output`: output name.
- `is_urgent`, `is_active`, `is_focused`: workspace state.
- `active_window_id`: numeric window id or `null`.

### windows and focused-window

`windows` returns an array. `focused-window` returns one object with the same shape:

```json
{
  "id": 2,
  "title": "tic-shell",
  "app_id": "com.mitchellh.ghostty",
  "pid": 54836,
  "workspace_id": 1,
  "is_focused": true,
  "is_floating": false,
  "is_urgent": false,
  "layout": {
    "pos_in_scrolling_layout": [1, 1],
    "tile_size": [1230.0, 920.0],
    "window_size": [1230, 920],
    "tile_pos_in_workspace_view": null,
    "window_offset_in_tile": [0.0, 0.0]
  },
  "focus_timestamp": {
    "secs": 28433,
    "nanos": 870263774
  }
}
```

Observed fields:

- `id`: numeric niri window identity.
- `title`: current window title; it can change often.
- `app_id`: Wayland app id.
- `pid`: process id.
- `workspace_id`: numeric niri workspace id.
- `is_focused`, `is_floating`, `is_urgent`: window state.
- `layout.tile_size` and `layout.window_size`: useful for reservation verification.

### layers

Shape is an array of layer-shell surfaces:

```json
[
  {
    "namespace": "noctalia-bar-content-Virtual-1",
    "output": "Virtual-1",
    "layer": "Top",
    "keyboard_interactivity": "None"
  }
]
```

Noctalia namespaces observed after the strut reload:

- `noctalia-background-Virtual-1`
- `noctalia-bar-content-Virtual-1`
- `noctalia-bar-exclusion-top-Virtual-1`
- `noctalia-dock-peek-Virtual-1`
- `noctalia-image-cache-renderer`
- `noctalia-wallpaper-Virtual-1`

### event-stream

`niri msg --json event-stream` emits newline-delimited JSON objects. Event variants observed:

- `WorkspacesChanged`
- `WindowsChanged`
- `KeyboardLayoutsChanged`
- `OverviewOpenedOrClosed`
- `ConfigLoaded`
- `CastsChanged`
- `WindowOpenedOrChanged`
- `WorkspaceActiveWindowChanged`
- `WindowFocusChanged`
- `WindowFocusTimestampChanged`

Example:

```json
{"ConfigLoaded":{"failed":false}}
```

## Sidebar Model Decisions

- Workspace row id: `niri:workspace:<workspace.id>`
- Window row id: `niri:window:<window.id>`
- Window-to-workspace relation: `window.workspace_id == workspace.id`
- Workspace display label: `workspace.name` when present, otherwise `workspace.idx`
- App indicators: group visible workspace windows by `window.app_id`
- Nested window rows: list windows where `window.workspace_id` matches the selected workspace
- Focused window: `window.is_focused == true` or `focused-window.id`
- Active workspace: `workspace.is_active == true`
- Focused workspace: `workspace.is_focused == true`

## Annotation Persistence

Workspace annotations are shell-owned metadata, not niri workspace names.

Initial persistence target:

```text
~/.local/state/lnx/workspaces.json
```

Initial shape:

```json
{
  "niri:workspace:1": {
    "annotation": "research",
    "updatedAt": "2026-05-06T00:00:00Z"
  }
}
```

## Phase 0 Exit Status

- niri struts were tested and found insufficient for a permanent left sidebar in the horizontal scrolling layout.
- The persistent niri strut was removed after the failed behavior was confirmed.
- A temporary Quickshell layer-shell panel with `ExclusionMode.Auto` kept the visible left rail unobscured during horizontal focus scrolling.
- The next implementation should use the sidebar layer-shell surface as the reservation source of truth.
- niri config validates.
- niri reload reports `ConfigLoaded` with `failed: false`.
- Noctalia layer surfaces remain visible in niri IPC.
- niri IPC has enough state for wide workspace cards, app indicators, focused state, and nested window lists.
