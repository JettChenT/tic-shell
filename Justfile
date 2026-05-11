set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

# Build the Rust workspace.
build:
    cargo build --workspace

# Build the release sidebar daemon used by the QML sidebar.
build-sidebar:
    cargo build --release --package tic-sidebar-core

# Check the Rust workspace.
check:
    cargo check --workspace

# Check only the Rust sidebar daemon.
check-sidebar:
    cargo check --package tic-sidebar-core

# Run the Quickshell/QML sidebar. Builds tic-sidebar-core if needed.
sidebar:
    bin/tic-sidebar start

# Alias for `just sidebar`.
run-sidebar: sidebar

# Stop the running Quickshell/QML sidebar.
stop-sidebar:
    bin/tic-sidebar stop

# Toggle the running sidebar.
toggle-sidebar:
    bin/tic-sidebar toggle

# Show the running sidebar.
show-sidebar:
    bin/tic-sidebar show

# Hide the running sidebar.
hide-sidebar:
    bin/tic-sidebar hide

# Toggle the Codex rail inside the running sidebar.
toggle-agent:
    bin/tic-sidebar toggle-agent

# Run the Bun ACP bridge tests.
test-agent:
    bun test tests/tic-codex-agent.test.mjs
