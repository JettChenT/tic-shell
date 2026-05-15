set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
tic_quickshell_bin := "~/.local/share/tic-shell/noctalia-qs/bin/qs"

default:
    @just --list

# Build the Rust workspace.
build:
    cargo build --workspace

# Check the Rust workspace.
check:
    cargo check --workspace

# Install the local CUA CLI.
install-cua:
    cargo install --path cua --force

# Run the integrated Noctalia left bar with the tic workspace rail.
sidebar:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar start

# Alias for `just sidebar`.
run-sidebar: sidebar

# Stop the integrated Noctalia/tic sidebar.
stop-sidebar:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar stop

# Toggle the running sidebar.
toggle-sidebar:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar toggle

# Show the running sidebar.
show-sidebar:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar show

# Hide the running sidebar.
hide-sidebar:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar hide

# Toggle the Codex rail inside the running sidebar.
toggle-agent:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar toggle-agent

# Run the Rust tic daemon directly for development.
daemon:
    TIC_SHELL_ROOT="$PWD" cargo run --bin tic-daemon

# Run daemon unit tests.
test-daemon:
    cargo test -p tic-daemon

# Restart the shell-owned daemon by restarting the Noctalia sidebar.
restart-daemon:
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar stop
    TIC_QUICKSHELL_BIN="${TIC_QUICKSHELL_BIN:-{{tic_quickshell_bin}}}" bin/tic-sidebar start

# Run the Bun ACP bridge tests.
test-agent:
    bun test tests/tic-codex-agent.test.mjs
