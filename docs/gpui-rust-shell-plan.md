# GPUI/Rust Agent Shell Plan

## Explicit Goal

The goal is to turn `tic-shell` into a fully Rust + GPUI niri shell whose primary interface is the existing sidebar concept.

This means two concrete things:

- Migrate the workspace bar and agent bar in this repo from Quickshell/QML to Rust + GPUI.
- Move `../gpui-shell` into this repo, then remove or disable the UI aspects that are duplicated by the sidebar, especially workspace indication and window indication.

The end product should be one integrated Rust application, not a Quickshell sidebar plus a separate GPUI shell. `tic-shell` should own the whole shell process, the sidebar UI, the agent-client-protocol client, niri integration, computer-use support, and the supporting desktop shell services.

## Finished Product

`tic-shell` should become a sidebar-centric shell for niri.

The left sidebar is the main desktop surface. It replaces the traditional workspace bar, active-window indicator, and agent panel with one coherent UI:

- a persistent left layer-shell surface
- exclusive-zone reservation as the only sidebar reservation mechanism
- a compact collapsed rail
- an expanded workspace rail
- an optional agent rail attached to the workspace rail
- live niri workspace state
- live niri window state
- workspace annotations
- per-workspace agent sessions
- tool-call, plan, transcript, and approval rendering
- keyboard and IPC controls for showing, hiding, and toggling the sidebar and agent pane

The rest of the shell should provide the system services and panels needed for daily use:

- launcher
- notification daemon and notification history
- system tray
- audio and brightness controls
- MPRIS/media controls
- network and Bluetooth panels
- battery and power/session controls
- OSD
- settings
- theming
- multi-monitor behavior

The product should replace the practical daily-use role of both Noctalia Shell and `gpui-shell`, while keeping the identity and workflow of `tic-shell`: niri-first, sidebar-first, and agent-native.

## Source Repositories

### `tic-shell`

`tic-shell` defines the product direction.

Keep and port:

- workspace sidebar behavior
- agent sidebar behavior
- three sidebar widths: compact, workspace-only, workspace plus agent
- layer-shell exclusive-zone behavior
- no niri strut for the sidebar
- workspace annotation model
- workspace-scoped agent sessions
- current Codex ACP behavior
- current `cua` computer-use behavior
- niri ground-truth notes and test expectations

The QML implementation is a reference for behavior, not the final runtime.

### `../gpui-shell`

`../gpui-shell` should be moved into this repo and become the Rust foundation.

Keep and adapt:

- GPUI app structure
- layer-shell window patterns
- service initialization model
- niri compositor backend
- D-Bus/system service integrations
- launcher implementation
- notification service
- tray service
- audio, network, Bluetooth, MPRIS, UPower, brightness, OSD, and theme services
- config and persistence scaffolding where useful

Remove, disable, or redesign UI that duplicates the sidebar:

- workspace bar widget
- active-window bar widget
- any window/workspace indication that competes with the sidebar
- bar layouts whose primary purpose is workspace navigation
- redundant launcher/workspace UI paths once the sidebar owns that workflow

The remaining `gpui-shell` UI should become supporting shell chrome and panels, not the primary workspace surface.

### `../noctalia-shell`

Noctalia is not the implementation base for this direction.

Use it only as a feature and UX benchmark for what a complete Wayland shell provides:

- breadth of shell features
- settings ideas
- panel and widget references
- niri behavior references
- plugin ecosystem ideas

Do not port Noctalia line-for-line into Rust.

## Product Shape

The final shell should feel like an agent-aware control surface for niri, not a normal status bar with a chat widget attached.

The workspace rail should answer:

- What workspaces exist?
- Which workspace is active?
- What windows are on each workspace?
- Which window is focused?
- What is the user’s note or intent for this workspace?
- What should happen if I click a workspace or window?

The agent rail should answer:

- Which workspace/session am I talking to?
- What did I ask?
- What is the agent doing now?
- What tools did it call?
- What permissions does it want?
- What did it change?
- What can I approve, deny, cancel, clear, or restart?

The system panels should answer:

- What is running?
- What needs attention?
- What system state changed?
- What quick controls do I need now?

## Rust Ownership

Rust should own all deep integrations.

The finished shell should not depend on QML/JavaScript for:

- niri state tracking
- niri action dispatch
- ACP protocol handling
- agent session state
- transcript state
- permission policy
- computer-use actions
- screenshots
- click/type/scroll actions
- file access mediation
- notification daemon behavior
- tray integration
- audio/network/MPRIS/power services
- persistence and migrations

GPUI should render state owned by Rust services. UI code should not become the source of truth for compositor state, agent state, or policy decisions.

## Target Rust Workspace

```text
crates/
  app/              GPUI application, window setup, shell routing, IPC entrypoints
  ui/               shared GPUI components, theme, icons, layout primitives
  shell-sidebar/    workspace rail, agent rail, sidebar layer-shell surface
  services/         niri, tray, notifications, audio, network, mpris, power
  agent/            ACP client, session store, transcript model, command model
  computer-use/     screenshots, window targeting, click/type/scroll actions
  policy/           approvals, scopes, audit log, permission decisions
  persistence/      settings, annotations, transcripts, migrations
```

The exact crate names can change, but the ownership boundaries should stay clear:

- UI renders state.
- Services collect and mutate system state.
- Agent owns ACP protocol state.
- Policy decides what the agent may do.
- Computer-use executes approved desktop actions.
- Persistence owns durable state and migration.

## Sidebar Responsibilities

The sidebar replaces duplicated workspace and window UI from `gpui-shell`.

Required workspace rail behavior:

- show all niri workspaces in stable order
- show active/focused/urgent/occupied state
- show windows nested under each workspace
- show app identity or fallback initials
- focus workspace on click
- focus window on click
- preserve workspace annotations
- support collapsed and expanded widths
- reserve exactly the visible width through layer-shell exclusive zone

Required agent rail behavior:

- one active session per workspace key
- transcript switches with workspace
- prompt queueing
- streaming assistant output
- tool-call rendering
- plan rendering
- slash command completion
- clear, new, and cancel controls
- explicit status for starting, ready, thinking, stopped, and error
- approval prompts for risky actions

## Agent And Computer-Use Model

The agent should be a first-class shell client, not an external process hidden behind UI glue.

ACP support should be implemented as a Rust service with typed state for:

- initialize
- session creation
- session close
- prompt submission
- cancellation
- available commands
- streaming updates
- tool calls
- plans
- permission requests
- filesystem read/write requests

Computer-use support should move from the standalone `cua` CLI into an internal Rust crate.

The shell should expose agent tools through a policy boundary:

- describe current workspace
- screenshot selected/focused window
- screenshot workspace
- focus workspace
- focus window
- click selected/focused window
- type into selected/focused window
- scroll selected/focused window
- inspect window/workspace metadata

Risky actions should require explicit policy decisions and should be auditable.

## UI Removed From `gpui-shell`

When `../gpui-shell` is moved into this repo, its bar should stop being the primary workspace control.

Remove or disable:

- workspace widget as a top/bottom/side bar item
- active-window widget as a bar item
- workspace launcher views that duplicate the sidebar’s workspace model
- any default layout where workspace navigation is mainly bar-based

Keep or adapt:

- launcher for applications, commands, settings, themes, and web search
- tray UI
- notifications UI
- control center panels
- OSD
- media controls
- battery/power UI
- network and Bluetooth UI
- settings and theme UI

The resulting UI should make the sidebar the obvious place for workspace and agent activity, while supporting panels handle system controls.

## Non-Goals

- Do not keep QML as a required runtime dependency.
- Do not preserve two competing workspace UIs.
- Do not use a niri strut for sidebar reservation.
- Do not expose unrestricted computer-use tools to the agent.
- Do not attempt a line-for-line Rust port of Noctalia.
- Do not make plugin marketplace parity a blocker for the first complete Rust shell.

## Success Criteria

The plan is successful when `tic-shell` can run as the only shell in a niri session and provide:

- GPUI/Rust sidebar replacing the current QML workspace and agent bars
- integrated `gpui-shell` services inside this repo
- no duplicated workspace/window indication outside the sidebar
- Rust ACP client with workspace-scoped sessions
- Rust computer-use service behind explicit policy
- reliable layer-shell reservation for the sidebar
- daily-use panels for launcher, tray, notifications, audio, network, media, power, OSD, settings, and theming
- no dependency on Quickshell for core shell behavior
