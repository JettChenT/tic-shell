set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

# Build the Rust workspace.
build:
    cargo build --workspace

# Check the Rust workspace.
check:
    cargo check --workspace

# Run the integrated Noctalia left bar with the tic workspace rail.
sidebar:
    bin/tic-sidebar start

# Alias for `just sidebar`.
run-sidebar: sidebar

# Stop the integrated Noctalia/tic sidebar.
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
