# Agent Guide

Repo-specific instructions for future coding agents working in `tic-shell`.

## Current Shape

- `shell/noctalia/` is the active Quickshell shell config.
- `shell/noctalia/Modules/TicWorkspace/` owns the tic workspace sidebar and embedded Codex pane inside Noctalia.
- `bin/tic-sidebar` starts and controls the Noctalia shell config through Quickshell IPC.
- `tic-daemon/` is the Rust daemon spawned by the Codex pane. It owns Codex ACP orchestration, the daemon MCP entrypoint, and Heart heartbeat loops.
- `bin/tic-codex-agent` is the older Bun bridge and should be treated as legacy reference code unless a task explicitly targets it.
- `tests/tic-codex-agent.test.mjs` covers the legacy Bun bridge.
- `cua/` is a Rust package for niri computer-use actions. Its library code is shared by Rust consumers; its CLI remains useful for direct checks.

## Local Commands

Prefer the repo `Justfile` when it has a recipe for the task. The recipes encode local defaults such as the installed Noctalia Quickshell binary path.

Cargo commands usually work directly on this machine without `nix develop`; verify before adding a Nix wrapper. As of 2026-05-15, both of these pass from the repo root without `nix develop`:

```sh
cargo check --workspace
cargo check --manifest-path cua/Cargo.toml
```

Avoid wrapping sidebar/Quickshell commands in raw `nix develop -c ...` by default. The dev shell takes `quickshell` from the `noctalia-qs` flake input, so entering it for sidebar work may try to build Quickshell from source, and raw `nix develop` may still not put the expected `qs` on `PATH` for the running shell workflow. Use the Justfile recipes, which set `TIC_QUICKSHELL_BIN` to `~/.local/share/tic-shell/noctalia-qs/bin/qs` unless the environment overrides it.

Preferred one-shot commands from the repo root:

```sh
just build
just check
just daemon
just test-daemon
just test-agent
just sidebar
just stop-sidebar
```

Equivalent direct commands:

```sh
cargo build --workspace
cargo check --workspace
cargo run --bin tic-daemon
bun test tests/tic-codex-agent.test.mjs
TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-$HOME/.local/share/tic-shell/noctalia-qs/bin/qs}" ./bin/tic-sidebar start
TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-$HOME/.local/share/tic-shell/noctalia-qs/bin/qs}" ./bin/tic-sidebar stop
```

Runtime sidebar controls:

```sh
just toggle-sidebar
just show-sidebar
just hide-sidebar
just toggle-agent
TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-$HOME/.local/share/tic-shell/noctalia-qs/bin/qs}" ./bin/tic-sidebar show-agent
TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-$HOME/.local/share/tic-shell/noctalia-qs/bin/qs}" ./bin/tic-sidebar hide-agent
```

Useful live checks in a niri session:

```sh
$HOME/.local/share/tic-shell/noctalia-qs/bin/qs list --all
niri msg layers
niri msg --json workspaces
niri msg --json windows
```

## Editing Rules

- Keep changes scoped. Check `git status --short` before and after edits.
- Do not overwrite local user edits.
- Prefer `rg` and `rg --files` for search.
- Use `apply_patch` for manual file edits.
- Keep documentation and scripts ASCII unless the edited file already uses non-ASCII for a clear reason.
- Do not add package managers or project scaffolding unless the task explicitly needs them.

## Sidebar Notes

- The active sidebar is the Noctalia integration.
- `bin/tic-sidebar` defaults `TIC_SHELL_SIDEBAR_CONFIG` to `shell/noctalia`.
- `TIC_SHELL_ROOT` defaults to the repo root and is passed to `tic-daemon`.
- `NOCTALIA_CONFIG_DIR` and `NOCTALIA_CACHE_DIR` default under `~/.config/tic-shell/noctalia` and `~/.cache/tic-shell/noctalia`.
- The sidebar reserves space through layer-shell exclusive-zone behavior. Do not add a niri left strut for the same reservation.
- The tic workspace layer namespaces include `tic-workspace-*` and `tic-workspace-exclusion-left-*`.
- QML uses Quickshell's native Niri service for workspace/window/title state and niri actions.
- Collapse/expand recentering lives in `shell/noctalia/Modules/TicWorkspace/Services/WorkspaceService.qml`.
- The Codex pane starts `cargo run --quiet --bin tic-daemon` from `TIC_SHELL_ROOT`.

## Rust Daemon Notes

- `tic-daemon` keeps the existing QML stdin/stdout JSON protocol for status, snapshots, workspace metadata, and events.
- The daemon advertises filesystem text capabilities and confines reads/writes to the generated workspace root.
- Permission requests are intentionally auto-allowed by selecting the strongest allow option available.
- The default adapter command installs/uses `@zed-industries/codex-acp@0.13.0` under `~/.tic/codex-acp` unless `TIC_CODEX_ACP_COMMAND` or config overrides it.
- Config lives at `~/.tic/config.toml` by default. Heart summaries and event logs live under `~/.tic/memory/` and `~/.tic/events.jsonl`.
- Codex sessions attach `tic-daemon mcp`, not `cua mcp`. The daemon MCP exposes `emit_event` and CUA compatibility tools.
- The Heart system listens to niri event stream and screenshot ticks. Window L1 and workspace L2 constants are configurable under `heartbeat.window-l1` and `heartbeat.workspace-l2`.

## Rust `cua` Notes

- The package lives under `cua/`; use `cargo check --manifest-path cua/Cargo.toml` from the repo root.
- The CLI depends on live niri IPC and may require `NIRI_SOCKET`, `XDG_RUNTIME_DIR`, and `WAYLAND_DISPLAY` outside a normal session.
- Non-intrusive screenshots should be preferred. `--intrusive-fallback` may focus windows and use `grim`.
- Input actions use compositor-native niri/tiri actions where available; failures may be live compositor or IPC environment issues rather than logic bugs.

## Verification

- For Noctalia/sidebar changes, run the relevant `just ...` sidebar recipe when available and verify with `niri msg layers`.
- For daemon changes, run `cargo check --workspace` and `cargo test -p tic-daemon`.
- For legacy Bun bridge changes, run `just test-agent`.
- For Rust CLI changes, run `cargo check --manifest-path cua/Cargo.toml`.
- For Rust workspace changes, run `just check` or `just build`.
- If a check cannot run because the environment lacks niri, Quickshell, or input permissions, report that explicitly.
