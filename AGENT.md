# Agent Guide

This file gives repo-specific instructions for future coding agents working in `tic-shell`.

## Project Shape

- `crates/app` is the Rust/GPUI application entrypoint. It opens the sidebar as a Wayland layer-shell surface and owns sidebar IPC.
- `crates/shell-sidebar` renders the GPUI workspace rail and Codex rail.
- `crates/services` owns niri workspace/window state and niri actions.
- `crates/agent` owns the typed Rust process wrapper around `bin/tic-codex-agent`.
- `crates/persistence` owns workspace annotations in the existing JSON shape.
- `shell/agent-sidebar/shell.qml` is the previous Quickshell entrypoint kept as a behavior reference.
- `bin/tic-sidebar` launches or controls the Rust/GPUI sidebar over IPC.
- `bin/tic-codex-agent` is an executable Bun script. It bridges sidebar JSON messages to a Codex ACP adapter over stdio and exports `createClient` for tests.
- `tests/tic-codex-agent.test.mjs` runs under `bun test` and imports `bin/tic-codex-agent` directly.
- `cua/` is a standalone Rust package for niri computer-use actions.

## Commands

Use these from the repo root:

```sh
cargo check --workspace
bun test tests/tic-codex-agent.test.mjs
cargo check --manifest-path cua/Cargo.toml
```

Useful runtime checks in a niri session:

```sh
bin/tic-sidebar start
bin/tic-sidebar toggle
bin/tic-sidebar toggle-agent
niri msg --json workspaces
niri msg --json windows
niri msg --json layers
```

## Editing Rules

- Keep changes scoped. This repo is small, and broad refactors are usually unnecessary.
- Do not overwrite local user edits. Check `git status --short` before and after changes.
- Prefer `rg`/`rg --files` for search.
- Use `apply_patch` for manual file edits.
- Keep documentation and scripts ASCII unless the edited file already uses non-ASCII for a clear reason.
- Do not add package managers or project scaffolding unless the task explicitly needs them.

## Sidebar Notes

- The sidebar reserves space through layer-shell exclusive-zone behavior. Do not add or recommend a niri left strut for the same sidebar reservation.
- The shell namespace is `tic-shell-agent-sidebar`.
- `TIC_SHELL_ROOT` controls where the Rust app looks for repo scripts.
- The Rust app starts `bun <TIC_SHELL_ROOT>/bin/tic-codex-agent`.
- The agent bridge defaults `TIC_CODEX_WORKDIR` to the user's home directory when started by the Rust sidebar.

## ACP Bridge Notes

- `bin/tic-codex-agent` should remain executable as a script and importable in tests.
- In tests, set `TIC_CODEX_AGENT_TEST=1` before importing to avoid starting the real adapter.
- The bridge advertises filesystem text capabilities and confines reads/writes to `TIC_CODEX_WORKDIR`.
- Permission requests are intentionally auto-allowed by selecting the strongest allow option available.
- The default adapter command uses `codex-acp` when present, otherwise `bunx --bun @zed-industries/codex-acp@0.13.0`.

## Rust `cua` Notes

- The package lives under `cua/`, so pass `--manifest-path cua/Cargo.toml` from the repo root.
- The CLI depends on live niri IPC and may require `NIRI_SOCKET`, `XDG_RUNTIME_DIR`, and `WAYLAND_DISPLAY` outside a normal session.
- Non-intrusive screenshots should be preferred. `--intrusive-fallback` may focus windows and use `grim`.
- Input actions use `uinput`; failures may be environment or permission related rather than logic bugs.

## Verification Expectations

- For Bun bridge changes, run `bun test tests/tic-codex-agent.test.mjs`.
- For Rust CLI changes, run `cargo check --manifest-path cua/Cargo.toml`.
- For Rust sidebar changes, run `cargo check --workspace`; when possible, run the sidebar in a niri session and inspect `niri msg --json layers`.
- For old QML reference changes, static checks are limited.
- If a check cannot run because the environment lacks niri, Quickshell, or input permissions, report that explicitly.
