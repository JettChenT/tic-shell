# tic-shell

`tic-shell` is a local Wayland/niri shell experiment. It currently contains:

- a Rust/GPUI left sidebar for niri workspaces and an embedded Codex ACP chat pane
- the previous Quickshell sidebar kept under `shell/agent-sidebar/` as a behavior reference
- a Bun-powered ACP client bridge used by the sidebar
- a Rust `cua` CLI for niri-focused computer-use actions such as workspace description, screenshots, clicks, typing, and scrolling

The project is intentionally small and local-first. Most runtime behavior assumes a niri Wayland session and the tools available on this machine.

## Repository Layout

```text
bin/
  tic-sidebar          Rust/GPUI launcher and IPC wrapper
  tic-codex-agent     Bun bridge between the sidebar and a Codex ACP adapter
crates/
  app/                GPUI application, layer-shell window, and sidebar IPC
  shell-sidebar/      GPUI workspace rail and Codex rail
  services/           niri workspace/window state and actions
  agent/              typed Rust wrapper around the Codex ACP bridge process
  persistence/        workspace annotation persistence
  ui/                 shared sidebar theme and sizing
cua/
  Cargo.toml          Rust CLI package
  src/main.rs         niri computer-use implementation
shell/agent-sidebar/
  shell.qml           previous Quickshell sidebar entrypoint
  Modules/            previous workspace and Codex pane composition
  Services/           previous Niri workspace, annotation, and Codex process state
  Widgets/            previous reusable sidebar controls and cards
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
- `cargo`/Rust for the GPUI sidebar and `cua` CLI
- `niri` for compositor IPC
- `codex-acp`, or `bunx` to run `@zed-industries/codex-acp`
- `grim` and Linux `uinput` support for some `cua` screenshot/input actions

The Rust sidebar uses GPUI's Wayland layer-shell support and talks to niri with `niri msg --json`.

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
- `TIC_SHELL_ROOT` can point the Rust sidebar at a different repo checkout.
- `TIC_CODEX_WORKDIR` controls the filesystem root exposed by the ACP bridge. The bridge rejects file reads/writes outside that root.
