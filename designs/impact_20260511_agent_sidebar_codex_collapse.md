# Agent Sidebar Codex Collapse Impact

## Change Summary

Add an independent Codex pane collapse mode to the existing Quickshell agent sidebar. The full sidebar collapse remains unchanged, while the expanded sidebar can now reserve only the workspace pane width when the Codex pane is hidden.

## Affected Modules/Files

- `shell/agent-sidebar/shell.qml`: adds Codex pane collapse state, derived rail width, UI toggle, and IPC helpers.

## Interface Changes

- Existing `sidebar.toggle`, `sidebar.reveal`, and `sidebar.hide` IPC methods are unchanged.
- New optional IPC methods: `sidebar.toggleAgent`, `sidebar.revealAgent`, and `sidebar.hideAgent`.

## Integration Points

- Panel width and layer-shell exclusive zone now bind to `railWidth`, which derives from both `sidebarCollapsed` and `agentPaneCollapsed`.
- Workspace header contains the Codex pane toggle when the full sidebar is expanded.
- The divider and agent pane are hidden only when `agentPaneCollapsed` or full-sidebar collapse is active.

## Risk Assessment

- Width binding mistakes could reserve the wrong layer-shell space.
- Header controls could crowd the workspace title at the 250px workspace width.
- Full-sidebar collapse could accidentally reset or conflict with Codex pane collapse state.

## Complexity Estimate

S
