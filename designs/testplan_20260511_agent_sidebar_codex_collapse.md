# Agent Sidebar Codex Collapse Test Plan

## Capability

The agent sidebar supports two independent collapse modes: full sidebar collapse to the compact rail, and Codex pane collapse that keeps the workspace pane visible while hiding only the Codex pane.

## Boundary

This change owns only the Quickshell sidebar UI state and layer-shell width reservation. It does not change Codex process behavior, workspace annotation storage, niri dispatch behavior, or the `tic-sidebar` CLI.

## Forbidden Zone

- Do not remove the existing full-sidebar compact rail behavior.
- Do not add niri struts or any second reservation mechanism.
- Do not stop or restart the Codex process solely because the pane is hidden.
- Do not change workspace card data or annotation persistence formats.

## Validation Plan

Must Have:

- Full sidebar collapse still shrinks to the compact rail and restores.
- Codex pane collapse hides only the Codex pane and divider.
- Workspace pane remains visible and interactive while the Codex pane is collapsed.
- Layer-shell exclusive zone follows the visible width in all three states: full rail, workspace-only rail, compact rail.
- Codex pane collapsed state can be restored without losing transcript state.

Need Have:

- IPC can independently toggle, reveal, and hide the Codex pane.
- Workspace header controls fit without overlapping at the workspace pane width.

Should Have:

- Codex pane collapse state persists while temporarily using full-sidebar collapse.

## Failure & Edge Cases

- Full sidebar collapsed while Codex pane is already collapsed: compact rail still wins.
- Full sidebar restored after Codex pane collapse: workspace-only rail is restored.
- Empty transcript or stopped Codex agent: pane can still be hidden and restored.

## Integration Tests

- Launch the sidebar and verify the panel width changes when the Codex toggle is clicked.
- Inspect `niri msg --json layers` and verify the `tic-shell-agent-sidebar` exclusive zone tracks the current visible width.
- Trigger `sidebar.toggleAgent`, `sidebar.hideAgent`, and `sidebar.revealAgent` through Quickshell IPC.

## E2E User Flows

E2E-1: Use workspace-only rail.

1. Start with the expanded two-pane sidebar.
2. Click the Codex toggle in the workspace header.
3. Confirm only the workspace pane remains visible.
4. Click workspace cards and edit annotations.
5. Restore Codex and confirm the transcript pane returns.

E2E-2: Combine both collapse modes.

1. Collapse the Codex pane.
2. Collapse the full sidebar.
3. Restore the full sidebar.
4. Confirm the workspace-only state is preserved.
5. Restore the Codex pane.

## E2E Coverage Matrix

| Capability | E2E Goals | Covered? |
| --- | --- | --- |
| Codex-only collapse | E2E-1 | yes |
| Workspace behavior preserved | E2E-1 | yes |
| Full collapse unchanged | E2E-2 | yes |
| Width reservation state | E2E-1, E2E-2 | yes |

## Environment Spec

- Type: local Wayland/niri session
- Exec: `qs -p /home/jettc/dev/tic-shell/shell/agent-sidebar --allow-duplicate`
- Workdir: `/home/jettc/dev/tic-shell`
