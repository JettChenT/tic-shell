# Agent chat web migration

## Goal

Move the Agent chat surface from hand-built QML widgets to an embedded
QtWebEngine view while preserving current behavior. QML remains responsible for
shell state, workspace/window discovery, and the `tic-codex-agent` process.
The web view owns transcript layout, composer editing, popups, inline badges,
and tool-card interaction.

## Existing QML chat surface

### `AgentPane.qml`

Responsibilities before migration:

- Shows the Agent pane header.
- Sends `new`, `clear`, and `cancel` controls.
- Displays stopped/error status.
- Hosts the transcript `Flickable`.
- Repeats `AgentEventBubble` for every `agentEvents` entry.
- Hosts `AgentPromptBox`.
- Auto-scrolls transcript to bottom on content changes.

Migration target:

- Becomes the QtWebEngine host.
- Loads `Web/index.html`.
- Pushes Agent snapshots into JavaScript.
- Receives prompt/control messages from JavaScript.

### `AgentEventBubble.qml`

Responsibilities before migration:

- Renders user, assistant, thought, thinking, tool, permission, and stderr events.
- Shows a spinner for `thinking`.
- Renders CUA tool calls with Noctalia-style icons.
- Shows screenshot/image tool output inline.
- Collapses long textual tool output, with expansion.
- Splits `tic://` reference markdown into text and badge segments.
- Renders workspace/window reference badges inside transcript messages.

Migration target:

- Implemented by `Web/agent-chat.js` and `Web/agent-chat.css`.
- Transcript event data remains unchanged.
- Tool expansion state is local to the web view.
- Reference markdown is still parsed from `[@label](tic://type/id)`.

### `AgentPromptBox.qml`

Responsibilities before migration:

- Owns text entry.
- Filters slash commands.
- Shows slash command popup.
- Detects `@` reference triggers.
- Shows workspace/window reference popup.
- Prioritizes focused and active-workspace windows through `shell.referenceItems`.
- Serializes selected references to `[@label](tic://type/id)`.
- Sends prompts through `shell.sendAgentPrompt`.
- Supports Enter to send, Shift+Enter for newline, Up/Down selection, Tab
  completion, and Escape dismissal.

Migration target:

- Implemented as a web `contenteditable` composer.
- Reference badges are inline `contenteditable=false` nodes in the text flow.
- Window badges use the app icon path supplied by QML when present.
- Serialized prompts preserve the existing `tic://` markdown contract.

### `ReferenceBadge.qml`

Responsibilities before migration:

- Renders workspace/window badges.
- Uses app icons for windows when available.
- Keeps labels elided inside a fixed maximum width.

Migration target:

- Implemented as `.reference-badge`.
- Used both in transcript and in the composer.

### `ReferencePopup.qml`

Responsibilities before migration:

- Shows all matching workspaces and windows.
- Uses window icons when available.
- Highlights the selected row.
- Exposes hover/click selection.

Migration target:

- Implemented as `.reference-popup`.
- Data continues to come from `shell.referenceItems("")`.
- Filtering happens in JavaScript over the full QML-supplied list.

### `SlashCommandPopup.qml`

Responsibilities before migration:

- Shows local and agent-supplied slash commands.
- Filters by command name and description.
- Exposes hover/click selection.

Migration target:

- Implemented as `.slash-popup`.
- Uses `shell.allAgentCommands()` from QML.

## Bridge contract

QML sends this state to the page:

- `status`
- `events`
- `commands`
- `references`
- `workspaceTitle`
- `theme`

The page sends these messages back through JavaScript console messages:

- `{ "type": "ready" }`
- `{ "type": "requestState" }`
- `{ "type": "prompt", "text": "..." }`
- `{ "type": "control", "action": "new|clear|cancel" }`

## Parity checklist

- Header title remains `Agent`.
- New, clear, and cancel controls are present.
- Error/stopped status is visible.
- Transcript auto-scrolls when near bottom.
- Thinking state shows a spinner.
- CUA tools use special icons.
- Screenshot tool output renders as an image.
- Textual tool output can be expanded.
- Transcript reference markdown renders as badges.
- Composer supports slash command popup.
- Composer supports `@` reference popup.
- Composer reference badges are inline with text.
- Window reference badges use app icons when available.
- Enter sends, Shift+Enter inserts newline.
- Tab/Enter complete selected popup entries.
- Up/Down move popup selection.
- Prompt serialization remains `[@label](tic://type/id)`.

## Remaining QML responsibilities

- `AgentBridge.qml` keeps process ownership and JSON line protocol.
- `TicWorkspacePanel.qml` keeps workspace/window/reference data helpers.
- `WorkspaceService.qml` keeps compositor and icon discovery.
