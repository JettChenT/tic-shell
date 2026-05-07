# Agent Sidebar Test Plan

## Capability

The standalone Quickshell sidebar renders a two-pane left rail. The workspace pane preserves existing workspace cards, annotations, app indicators, and nested window lists. The agent pane shows ACP agent status, current desktop scope, permission mode controls, transcript entries, tool call cards, approval prompts, a persistent pause/resume control, and a bottom input box.

## Boundary

This phase owns only the shell UI and local interaction model. It does not implement ACP stdio JSON-RPC, spawn an agent process, expose MCP tools, or implement a desktop bridge.

## Forbidden Zone

- Do not fork niri or Noctalia.
- Do not add niri struts alongside the layer-shell exclusive zone.
- Do not grant real compositor control from the agent panel in this phase.
- Do not remove or rewrite the existing workspace annotation state format.

## Validation Plan

Must Have:

- Expanded sidebar reserves and renders a workspace pane plus an agent pane.
- Collapsed sidebar still shrinks to the compact rail and can be restored.
- Workspace cards remain clickable, editable, and live-updated from niri state.
- Agent pane shows focused workspace/window scope when niri state is available.
- Permission mode can be changed among Ask, Observe, and Control.
- Pause/resume control is always visible in the agent pane.
- Sending text adds a user transcript entry and a local assistant status entry.
- Approval prompt buttons record an allow/deny result in the transcript.

Need Have:

- Tool call rows are visually distinct from chat messages.
- Empty input submission is ignored.
- Long transcript content scrolls instead of resizing the panel.

Should Have:

- The wider rail uses a single layer-shell exclusive zone as the reservation source.
- The local stub text makes the missing ACP runtime explicit without blocking UI use.

## Failure & Edge Cases

- No focused window: scope falls back to focused/active workspace.
- No workspace rows yet: scope displays unavailable instead of throwing QML errors.
- Empty transcript/input state: panel still renders stable controls.
- Collapsed mode: agent pane is hidden and no horizontal overflow is visible.

## Integration Tests

- Start the shell, inspect `niri msg --json layers`, and verify the namespace is still `tic-shell-agent-sidebar`.
- Focus different niri windows/workspaces and verify scope text updates in the panel.
- Edit a workspace annotation, restart the shell, and verify the annotation remains.

## E2E User Flows

E2E-1: Use normal desktop with the two-pane rail.

1. Start the sidebar.
2. Confirm application windows tile to the right of the rail.
3. Switch workspaces from the workspace pane.
4. Confirm the agent pane remains visible and scope updates.

E2E-2: Prepare an agent request without real ACP execution.

1. Type a prompt into the agent input.
2. Press Enter.
3. Confirm the transcript shows the user prompt.
4. Confirm the panel adds a local assistant entry explaining that ACP runtime wiring is pending.

E2E-3: Handle a permission prompt.

1. Click Allow or Deny on the visible approval prompt.
2. Confirm the prompt resolves.
3. Confirm the transcript records the chosen outcome.

## E2E Coverage Matrix

| Capability | E2E Goals | Covered? |
| --- | --- | --- |
| Two-pane rail rendering | E2E-1 | yes |
| Workspace behavior preserved | E2E-1 | yes |
| Agent prompt entry | E2E-2 | yes |
| Permission prompt handling | E2E-3 | yes |
| Pause/resume visibility | E2E-1 | yes |

## Environment Spec

- Type: local Wayland/niri session
- Exec: `/nix/store/iyi8sawnx7w04rq2sva9mxl7bm1xfkjd-quickshell-2026-05-03_d3e26cc/bin/qs -p /home/jettc/osdev/tic-shell/shell/agent-sidebar --allow-duplicate`
- Workdir: `/home/jettc/osdev/tic-shell`
