# tic-shell

`tic-shell` is a local Wayland/niri shell experiment. It currently contains:

- a Quickshell/QML left sidebar for niri workspaces and an embedded Codex ACP chat pane
- native Quickshell/Niri bindings for workspace, active-window, and window-title state
- a Bun-powered ACP client bridge used by the sidebar
- a Rust `cua` CLI for niri-focused computer-use actions such as workspace description, screenshots, clicks, typing, and scrolling

The project is intentionally small and local-first. Most runtime behavior assumes a niri Wayland session and the tools available on this machine.

## Repository Layout

```text
bin/
  tic-sidebar          Quickshell launcher and IPC wrapper
  tic-codex-agent     Bun bridge between the sidebar and a Codex ACP adapter
cua/
  Cargo.toml          Rust CLI package
  src/main.rs         niri computer-use implementation
shell/agent-sidebar/
  shell.qml           Quickshell sidebar entrypoint
  Modules/            workspace and Codex pane composition
  Services/           native Niri workspace state, annotation, and Codex process state
  Widgets/            reusable sidebar controls and cards
tests/
  tic-codex-agent.test.mjs
designs/
  implementation notes and test plans
docs/
  niri ground-truth notes
```

## Requirements

Runtime requirements depend on the component:

- `bun` for `bin/tic-codex-agent` and its tests
- `cargo`/Rust for the `cua` CLI
- `qs` or `quickshell` for the QML sidebar
- `niri` for compositor IPC
- `codex-acp`, or `bunx` to run `@zed-industries/codex-acp`
- `grim` and Linux `uinput` support for some `cua` screenshot/input actions

The QML sidebar uses Quickshell's Wayland layer-shell support. Workspace/window state comes from Quickshell's native Niri service.

## Development Commands

Run the Bun ACP bridge tests:

```sh
bun test tests/tic-codex-agent.test.mjs
```

Check the Rust CLI:

```sh
cargo check --manifest-path cua/Cargo.toml
```

Check the Rust workspace:

```sh
cargo check --workspace
```

Run the sidebar:

```sh
bin/tic-sidebar start
```

Toggle the sidebar after it is running:

```sh
bin/tic-sidebar toggle
```

Toggle the Codex rail after the sidebar is running:

```sh
bin/tic-sidebar toggle-agent
```

Run the ACP bridge directly:

```sh
bun bin/tic-codex-agent
```

Override the ACP adapter command:

```sh
TIC_CODEX_ACP_COMMAND="codex-acp" bun bin/tic-codex-agent
```

## Rust `cua` CLI

The Rust CLI talks to niri through `niri msg` and prints JSON.

```sh
cargo run --manifest-path cua/Cargo.toml -- describe-workspace
cargo run --manifest-path cua/Cargo.toml -- screenshot-window <window-id>
cargo run --manifest-path cua/Cargo.toml -- click <window-id> <x> <y>
cargo run --manifest-path cua/Cargo.toml -- type-text <window-id> "hello"
cargo run --manifest-path cua/Cargo.toml -- scroll <window-id> down 5
```

Screenshot commands first try niri's non-intrusive screenshot action. Pass `--intrusive-fallback` before the subcommand to focus the target window and use `grim` if needed:

```sh
cargo run --manifest-path cua/Cargo.toml -- --intrusive-fallback describe-workspace
```

If the process is outside the compositor environment, set `NIRI_SOCKET`, `XDG_RUNTIME_DIR`, and `WAYLAND_DISPLAY` explicitly.

## Notes

- The sidebar uses a layer-shell exclusive zone as its reservation source. Do not pair it with a niri left strut, because that double-reserves horizontal space.
- Workspace annotations are persisted at `~/.local/state/lnx/workspaces.json`.
- `TIC_SHELL_ROOT` can point the QML sidebar at a different repo checkout.
- `TIC_CODEX_WORKDIR` controls the filesystem root exposed by the ACP bridge. The bridge rejects file reads/writes outside that root.
