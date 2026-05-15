# Collaboration V1 Progress

This document captures the current state of the two-machine collaboration demo
work across `tic-shell` and `tiri`, and the remaining plan to turn it into a
native remote-window experience.

## Goal

The target V1 is:

- Two machines are both running `tiri` and the tic sidebar.
- A user can choose to share a specific local window, not an entire desktop or
  workspace.
- The collaborator sees shared windows in the tic/sidebar model.
- The collaborator can open a shared window locally and interact with it through
  a dedicated remote seat.
- Video transport uses PipeWire and WebRTC, not periodic screenshots.
- The design is network-agnostic. Tailscale can be one transport for peer
  reachability, but the product model should not be Tailscale-specific.

## Current Implementation

### `tiri` IPC and compositor state

The `tiri` checkout has new IPC types and actions for collaboration metadata:

- `Request::RemoteWindows`
- `Request::SharedWindowStreams`
- `Action::ShareWindowStream`
- `Action::StopWindowStream`
- `Action::OpenRemoteWindow`
- `Action::FocusRemoteWindow`
- `Action::CloseRemoteWindow`
- `RemoteWindow`
- `SharedWindowStream`

`src/niri.rs` now stores two in-memory maps:

- `remote_windows`
- `shared_window_streams`

The host-side `ShareWindowStream` action validates that the local window exists,
records the stream metadata, and sets the compositor dynamic cast target to that
window. That connects the collaboration flow to the existing tiri PipeWire cast
target machinery.

The client/server IPC path can now query:

```sh
niri msg --json remote-windows
niri msg --json shared-window-streams
```

At this stage, these remote windows are metadata objects, not yet first-class
layout/render elements.

### `tic-daemon collab`

`tic-daemon` now has a `collab` command group in:

```text
tic-daemon/src/collab.rs
```

Available commands:

```sh
tic-daemon collab share-window --id <window-id> [--stream-id <stream-id>]
tic-daemon collab stop-window --id <window-id>
tic-daemon collab open-remote-window --peer-id <peer> --remote-window-id <id> --title <title> --stream-id <stream>
tic-daemon collab focus-remote-window --id <local-remote-window-id>
tic-daemon collab close-remote-window --id <local-remote-window-id>
tic-daemon collab remote-windows
tic-daemon collab shared-window-streams
tic-daemon collab publish-window --id <window-id> [--stream-id <stream-id>]
tic-daemon collab view-window --peer-id <peer> --remote-window-id <id> --title <title> --stream-id <stream> --producer-peer-id <webrtcsink-peer-id>
```

The simple metadata commands shell out to `niri msg` for now. This keeps the
demo path small and lets the daemon drive the compositor without inventing a new
local control protocol yet.

### Real media path

The current media path uses:

- XDG Desktop Portal ScreenCast API through the Rust `ashpd` crate.
- PipeWire stream node returned by the portal.
- GStreamer `pipewiresrc` to read frames.
- GStreamer `webrtcsink` to publish the stream.
- GStreamer `webrtcsrc` to receive the stream.

Relevant references:

- XDG ScreenCast portal:
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
- `ashpd` ScreenCast docs:
  https://docs.rs/ashpd/0.13.11/ashpd/desktop/screencast/
- GStreamer rswebrtc:
  https://gstreamer.freedesktop.org/documentation/rswebrtc/

The `tic-shell` dev shell now includes GStreamer and the plugin set needed for
`pipewiresrc`, `webrtcsink`, and `webrtcsrc`. It also sets `GST_PLUGIN_PATH`
because the rswebrtc plugin was not discoverable reliably without it.

## Demo Commands

On the sharing machine:

```sh
nix develop -c cargo run --bin tic-daemon -- collab publish-window \
  --id <window-id> \
  --stream-id demo-window
```

This command:

1. Registers the local window as a shared stream in tiri.
2. Sets the dynamic cast target to the selected window.
3. Opens a ScreenCast portal session.
4. Starts a GStreamer WebRTC publisher.

On the viewing machine:

```sh
nix develop -c cargo run --bin tic-daemon -- collab view-window \
  --peer-id host \
  --remote-window-id <host-window-id> \
  --title "Remote Terminal" \
  --stream-id demo-window \
  --producer-peer-id <peer-id-from-host-output> \
  --signaller-uri ws://<host>:8443
```

This command:

1. Registers remote-window metadata in local tiri.
2. Starts a GStreamer WebRTC receiver.
3. Renders the received stream into a local Wayland window via `waylandsink`.

## Verification So Far

The current code compiles:

```sh
# tic-shell
cargo check --workspace

# tiri
nix develop -c cargo check
```

GStreamer element availability was checked with `gst-inspect-1.0` for:

- `pipewiresrc`
- `webrtcsink`
- `webrtcsrc`

## Known Gaps

### Viewer rendering is not compositor-native yet

The receive path currently ends in:

```text
webrtcsrc ! queue ! videoconvert ! waylandsink
```

That gives us a real WebRTC stream, but it creates a normal local Wayland
client window. The remote window metadata exists in tiri, but decoded frames are
not yet rendered by tiri as a native remote-window texture.

This is the biggest remaining gap relative to the desired "remote tiri windows"
experience.

### Input routing is not implemented

The current work does not yet create a remote seat or forward pointer/keyboard
events to the host. It only lays the state and media foundation.

The expected V1 path is:

- Viewer focuses a remote window.
- tiri routes input events for that focused remote window to a collaboration
  control channel.
- Host receives those events on a dedicated remote seat.
- Host injects them into the shared window without stealing the local user's
  physical seat.

### Peer discovery and session orchestration are still manual

The current commands assume the operator knows:

- host address
- stream id
- remote window id
- WebRTC producer peer id
- signaller URI

The tic sidebar is not yet showing collaborator workspaces or shared windows.

### Portal source selection is still a rough edge

`publish-window --id` records the desired tiri window id and sets the dynamic
cast target, but the actual ScreenCast source is still selected through the
portal path. The intended final shape is a tiri-owned capture API that returns a
PipeWire node for an exact compositor window without requiring picker ambiguity.

## Future Plan

### 1. Make remote windows first-class tiri layout elements

Add a compositor-side representation for remote windows that participates in the
normal workspace layout enough to support:

- placement in workspaces
- focus
- close
- title/app metadata
- damage tracking
- input hit testing
- render hooks

The current `RemoteWindow` map should become backing state for that layout
object, not just IPC-visible metadata.

### 2. Replace viewer `waylandsink` with compositor-owned frame ingestion

The next media step is to terminate WebRTC decode into a texture path owned by
tiri.

Likely options:

- Run a small GStreamer receiver component that exports decoded frames as
  DMA-BUFs or shared memory to tiri.
- Embed the receiver in a Rust helper process and pass frame handles over a
  local IPC protocol.
- Move the receiver directly into tiri if lifecycle and dependency costs are
  acceptable.

The preferred V1 shape is probably a helper process first. It keeps heavy
GStreamer/WebRTC dependencies out of the compositor while still giving tiri
ownership of the final rendered remote surface.

### 3. Add a typed collaboration control protocol

The daemon needs a peer-to-peer control channel separate from the media channel.
It should carry:

- peer identity
- shared workspace list
- shared window list
- stream offers/answers or signaller metadata
- focus requests
- remote input events
- clipboard/file-transfer capability flags later

This protocol should be transport-agnostic. Tailscale can provide addressing,
ACLs, and encryption for a trusted deployment, but the protocol should also work
over localhost, LAN, SSH tunnel, or another overlay network.

### 4. Implement remote seat input

On the viewer:

- When a remote window is focused, pointer and keyboard events should be
  serialized onto the control channel instead of being delivered to a local
  Wayland client.

On the host:

- tiri should create or reuse a collaboration seat scoped to the remote peer.
- Pointer motion, button, scroll, key, and text events should target the shared
  window.
- The local physical seat should stay independent.

This is the step that turns "watch a stream" into "use another person's window".

### 5. Sidebar integration

The tic sidebar should eventually show:

- collaborators
- shared workspaces
- shared windows inside those workspaces
- stream status
- focus/open/close controls
- sharing controls for local windows

The sidebar should not own core collaboration state. It should observe daemon
and compositor state and call explicit daemon/compositor actions.

### 6. Improve capture API

The current portal-based publisher is useful for validating the media path, but
the final product should expose a compositor-owned exact-window capture API.

Desired host call:

```text
share window <local-window-id> -> PipeWire node + stream metadata
```

That avoids manual portal picking and makes the product behavior deterministic.

### 7. Package the two-machine demo

Once compositor-native receive and input are in place, add a small demo runner:

```sh
tic-daemon collab serve --listen <addr>
tic-daemon collab connect --peer <addr>
tic-daemon collab share-window --id <window-id>
tic-daemon collab open-window --peer <peer> --remote-window-id <id>
```

For V1 this can assume:

- both machines already run compatible `tiri`
- both machines already run tic sidebar
- no polished onboarding
- explicit peer addresses
- no NAT traversal beyond the chosen network layer

## Recommended Next Task

The next implementation task should be compositor-native remote-window rendering.
Without that, the demo has real streaming, but not the integrated "remote window
inside tiri" experience.

The smallest useful target is:

1. Keep the GStreamer WebRTC receiver in a helper process.
2. Export decoded frames to tiri through a simple local protocol.
3. Render those frames inside a focused `RemoteWindow` object in tiri.
4. Leave remote input for the following task.
