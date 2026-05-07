# Agent Sidebar Impact

## Change Summary

Extend the existing standalone Quickshell left sidebar from a workspace-only rail into the two-tier rail described in the referenced `~/dev/lnx/docs` plan: workspace column on the left, ACP agent panel on the right. This phase adds the agent panel UI and local interaction model only; it does not start a real ACP subprocess or desktop bridge.

## Affected Modules/Files

- `shell/agent-sidebar/shell.qml`: expand panel width, split the surface into workspace and agent panes, add ACP agent status, scope, permission mode, transcript, tool call, approval prompt, pause/resume, and input controls.
- `designs/testplan_20260506_agent_sidebar.md`: feature-level checkpoints for the new sidebar behavior.

## Interface Changes

- Existing IPC target `sidebar` keeps `toggle`, `reveal`, and `hide`.
- New optional IPC functions on `sidebar`: `pauseAgent()` and `resumeAgent()`.
- No changes to `bin/tic-sidebar`.

## Integration Points

- Uses existing `Niri` workspace/window state to display the agent scope.
- Uses existing layer-shell exclusive zone behavior to reserve the wider two-pane sidebar.
- Keeps workspace annotation persistence in `~/.local/state/lnx/workspaces.json`.

## Risk Assessment

- Wider exclusive zone can over-reserve screen space if the compositor or user expects the old 250px rail.
- QML layout changes could regress workspace card sizing or collapsed mode.
- The panel is an ACP-ready local stub, so users may expect real agent execution before ACP service integration exists.

## Complexity Estimate

M. One QML module is affected, but the layout and user-facing behavior are broader than a small isolated addition.
