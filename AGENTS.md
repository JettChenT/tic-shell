# Agent Guide

Repo-specific instructions for future coding agents working in `tic-shell`.

## Current Shape

- `shell/noctalia/` is the active Quickshell shell config.
- `shell/noctalia/Modules/TicWorkspace/` owns the tic workspace sidebar and embedded Codex pane inside Noctalia.
- `bin/tic-sidebar` starts and controls the Noctalia shell config through Quickshell IPC.
- `bin/tic-codex-agent` is an executable Bun script used by the Codex pane. It also exports `createClient` for tests.
- `tests/tic-codex-agent.test.mjs` runs under `bun test` and imports `bin/tic-codex-agent` directly.
- `cua/` is a standalone Rust package for niri computer-use actions.
- Shell UI work belongs under `shell/`; Rust code is limited to `cua/` unless a task explicitly adds otherwise.

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
just test-agent
just sidebar
just stop-sidebar
```

Equivalent direct commands:

```sh
cargo build --workspace
cargo check --workspace
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
- `TIC_SHELL_ROOT` defaults to the repo root and is used by the Codex pane to find `bin/tic-codex-agent`.
- `NOCTALIA_CONFIG_DIR` and `NOCTALIA_CACHE_DIR` default under `~/.config/tic-shell/noctalia` and `~/.cache/tic-shell/noctalia`.
- The sidebar reserves space through layer-shell exclusive-zone behavior. Do not add a niri left strut for the same reservation.
- The tic workspace layer namespaces include `tic-workspace-*` and `tic-workspace-exclusion-left-*`.
- QML uses Quickshell's native Niri service for workspace/window/title state and niri actions.
- Collapse/expand recentering lives in `shell/noctalia/Modules/TicWorkspace/Services/WorkspaceService.qml`.
- The Codex pane starts `bun <TIC_SHELL_ROOT>/bin/tic-codex-agent`.

## ACP Bridge Notes

- `bin/tic-codex-agent` should remain executable as a script and importable in tests.
- In tests, set `TIC_CODEX_AGENT_TEST=1` before importing to avoid starting the real adapter.
- The bridge advertises filesystem text capabilities and confines reads/writes to `TIC_CODEX_WORKDIR`.
- Permission requests are intentionally auto-allowed by selecting the strongest allow option available.
- The default adapter command uses `codex-acp` when present, otherwise `bunx --bun @zed-industries/codex-acp@0.13.0`.

## Rust `cua` Notes

- The package lives under `cua/`; use `cargo check --manifest-path cua/Cargo.toml` from the repo root.
- The CLI depends on live niri IPC and may require `NIRI_SOCKET`, `XDG_RUNTIME_DIR`, and `WAYLAND_DISPLAY` outside a normal session.
- Non-intrusive screenshots should be preferred. `--intrusive-fallback` may focus windows and use `grim`.
- Input actions use `uinput`; failures may be environment or permission related rather than logic bugs.

## Verification

- For Noctalia/sidebar changes, run the relevant `just ...` sidebar recipe when available and verify with `niri msg layers`.
- For Bun bridge changes, run `just test-agent`.
- For Rust CLI changes, run `cargo check --manifest-path cua/Cargo.toml`.
- For Rust workspace changes, run `just check` or `just build`.
- If a check cannot run because the environment lacks niri, Quickshell, or input permissions, report that explicitly.
