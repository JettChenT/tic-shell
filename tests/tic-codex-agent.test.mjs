import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test, { after } from "node:test";
import { pathToFileURL } from "node:url";

process.env.TIC_CODEX_AGENT_TEST = "1";

const moduleUrl = pathToFileURL(path.resolve("bin/tic-codex-agent")).href;
const { createClient } = await import(moduleUrl);
const testRoot = await mkdtemp(path.join(os.tmpdir(), "tic-codex-agent-tests-"));
let nextClientRoot = 1;

after(async () => {
  await rm(testRoot, { recursive: true, force: true });
});

function makeClient(options = {}) {
  const messages = [];
  const fake = {
    writes: [],
    stdin: {
      write: line => fake.writes.push(JSON.parse(line)),
    },
  };
  const client = createClient({
    child: fake,
    emitMessage: message => messages.push(message),
    workspaceRoot: path.join(testRoot, `client-${nextClientRoot++}`),
    ...options,
  });
  return { client, messages, writes: fake.writes };
}

function latestSnapshot(messages) {
  return messages.filter(message => message.type === "snapshot").at(-1).events;
}

function respondTo(writes, client, method, result) {
  const request = writes.findLast(write => write.method === method);
  client.handleAgentMessage({ id: request.id, result });
  return request;
}

test("initialize advertises filesystem text capabilities", () => {
  const { client, writes } = makeClient();

  client.initialize();

  assert.equal(writes[0].method, "initialize");
  assert.deepEqual(writes[0].params.clientCapabilities.fs, {
    readTextFile: true,
    writeTextFile: true,
  });
});

test("assistant token chunks update one transcript entry", () => {
  const { client, messages } = makeClient();

  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "hel" } } },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "lo" } } },
  });

  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.body]), [["assistant", "hello"]]);
});

test("lifecycle and metadata updates do not create transcript entries", () => {
  const { client, messages, writes } = makeClient();

  client.initialize();
  client.handleAgentMessage({
    id: writes[0].id,
    result: { agentInfo: { title: "Codex ACP" } },
  });
  client.handleAgentMessage({
    id: writes[1].id,
    result: { sessionId: "session-1" },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "available_commands_update", availableCommands: [] } },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "current_mode_update", modeId: "default" } },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "some_future_metadata_update", content: { type: "text", text: "noise" } } },
  });
  client.handleAgentMessage({
    id: writes.find(write => write.method === "session/prompt")?.id,
    result: { stopReason: "end_turn" },
  });
  client.handleExit(0, null);

  assert.equal(messages.some(message => message.type === "snapshot"), false);
});

test("thought chunks render as thinking blocks and do not merge into assistant chunks", () => {
  const { client, messages } = makeClient();

  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "thinking" } } },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "answer" } } },
  });

  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.title, entry.body]), [
    ["thinking", "Thinking", "thinking"],
    ["assistant", "Codex", "answer"],
  ]);
});

test("tool updates replace the existing tool entry", () => {
  const { client, messages } = makeClient();

  client.handleAgentMessage({
    method: "session/update",
    params: {
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "Read file",
        status: "pending",
      },
    },
  });
  client.handleAgentMessage({
    method: "session/update",
    params: {
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        fields: { status: "completed", content: { type: "text", text: "done" } },
      },
    },
  });

  const entries = latestSnapshot(messages);
  assert.equal(entries.length, 1);
  assert.equal(entries[0].id, "tool:tool-1");
  assert.equal(entries[0].body, "completed\ndone");
});

test("echoed user chunks are ignored when prompt was already added locally", () => {
  const { client, messages } = makeClient();

  client.queuePrompt("hello world");
  client.handleAgentMessage({
    method: "session/update",
    params: { update: { sessionUpdate: "user_message_chunk", content: { type: "text", text: "hello" } } },
  });

  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.body]), [["user", "hello world"]]);
});

test("permission requests auto-select the strongest allow option", async () => {
  const { client, writes } = makeClient();

  client.handleAgentMessage({
    jsonrpc: "2.0",
    id: 7,
    method: "session/request_permission",
    params: {
      toolCall: { title: "Edit" },
      options: [
        { optionId: "reject", kind: "reject_once", name: "Reject" },
        { optionId: "allow", kind: "allow_always", name: "Allow always" },
      ],
    },
  });

  assert.deepEqual(writes.at(-1), {
    jsonrpc: "2.0",
    id: 7,
    result: { outcome: { outcome: "selected", optionId: "allow" } },
  });
});

test("filesystem handlers stay inside workspace root", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "tic-acp-"));
  try {
    const { client, writes } = makeClient({ workspaceRoot: root });
    client.prepareWorkspaceRoot(client.workspaces.get("workspace:default"));
    await client.handleWriteTextFile({ path: "nested/file.txt", content: "a\nb\nc\n" });

    const workspaceRoot = path.join(root, "workspace-default");
    assert.equal(await readFile(path.join(workspaceRoot, "nested/file.txt"), "utf8"), "a\nb\nc\n");
    assert.deepEqual(await client.handleReadTextFile({ path: "nested/file.txt", line: 2, limit: 1 }), { content: "b\n" });
    assert.throws(() => client.resolveWorkspacePath("../outside.txt"), /outside TIC_CODEX_WORKDIR/);

    await client.handleAgentMessage({
      jsonrpc: "2.0",
      id: 11,
      method: "fs/read_text_file",
      params: { path: "nested/file.txt", line: 1, limit: 1 },
    });
    assert.deepEqual(writes.at(-1), {
      jsonrpc: "2.0",
      id: 11,
      result: { content: "a\n" },
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("workspace setup creates per-workspace AGENTS instructions", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "tic-workspaces-"));
  try {
    const { client } = makeClient({ workspaceRoot: root });

    client.setActiveWorkspace("niri:workspace:2", "2");
    const state = client.workspaces.get("niri:workspace:2");
    const workspaceRoot = client.prepareWorkspaceRoot(state);
    const agentsMd = await readFile(path.join(workspaceRoot, "AGENTS.md"), "utf8");

    assert.equal(workspaceRoot, path.join(root, "workspace-2"));
    assert.match(agentsMd, /Numeric workspace id\/index: 2/);
    assert.match(agentsMd, /Never pass values like `niri:workspace:1` as `workspace_id`/);
    assert.match(agentsMd, /`cua` MCP server is attached/);
    assert.match(agentsMd, /Do not run the legacy `cua \.\.\.` shell CLI/);
    assert.match(agentsMd, /`view-window` captures a single window/);
    assert.match(agentsMd, /`describe-workspace` returns window metadata plus a composite screenshot/);
    assert.match(agentsMd, /window-relative screenshot\/image pixel coordinates/);
    assert.match(agentsMd, /Do not call `describe-workspace` as a reflex/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("workspace sessions use distinct ad-hoc folders as cwd", () => {
  const root = path.join(testRoot, "session-cwds");
  const { client, writes } = makeClient({ workspaceRoot: root });

  client.initialize();
  respondTo(writes, client, "initialize", {});
  assert.equal(writes.findLast(write => write.method === "session/new").params.cwd, path.join(root, "workspace-default"));

  client.setActiveWorkspace("niri:workspace:1", "1");
  assert.equal(writes.findLast(write => write.method === "session/new").params.cwd, path.join(root, "workspace-1"));

  client.setActiveWorkspace("niri:workspace:2", "2");
  assert.equal(writes.findLast(write => write.method === "session/new").params.cwd, path.join(root, "workspace-2"));
});

test("workspace sessions attach the CUA MCP server", () => {
  const root = path.join(testRoot, "session-mcp");
  const { client, writes } = makeClient({ workspaceRoot: root });

  client.initialize();
  respondTo(writes, client, "initialize", {});
  const defaultSession = writes.findLast(write => write.method === "session/new");
  assert.equal(defaultSession.params.mcpServers.length, 1);
  assert.equal(defaultSession.params.mcpServers[0].name, "cua");
  assert.ok(path.isAbsolute(defaultSession.params.mcpServers[0].command));
  assert.deepEqual(defaultSession.params.mcpServers[0].args.slice(0, 4), [
    "run",
    "--quiet",
    "--manifest-path",
    path.resolve("cua/Cargo.toml"),
  ]);

  client.setActiveWorkspace("niri:workspace:2", "2");
  const workspaceSession = writes.findLast(write => write.method === "session/new");
  assert.deepEqual(workspaceSession.params.mcpServers[0].env.find(item => item.name === "CUA_WORKSPACE_ID"), {
    name: "CUA_WORKSPACE_ID",
    value: "2",
  });
});

test("workspaces get independent ACP sessions and transcripts", () => {
  const { client, messages, writes } = makeClient();

  client.initialize();
  respondTo(writes, client, "initialize", { agentInfo: { title: "Codex ACP" } });
  respondTo(writes, client, "session/new", { sessionId: "default-session" });

  client.setActiveWorkspace("niri:workspace:1", "1");
  respondTo(writes, client, "session/new", { sessionId: "session-1" });
  client.queuePrompt("one", "niri:workspace:1", "1");
  assert.equal(writes.at(-1).method, "session/prompt");
  assert.equal(writes.at(-1).params.sessionId, "session-1");
  client.handleAgentMessage({
    method: "session/update",
    params: {
      sessionId: "session-1",
      update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "answer one" } },
    },
  });

  client.setActiveWorkspace("niri:workspace:2", "2");
  respondTo(writes, client, "session/new", { sessionId: "session-2" });
  assert.deepEqual(latestSnapshot(messages), []);
  client.queuePrompt("two", "niri:workspace:2", "2");
  assert.equal(writes.at(-1).params.sessionId, "session-2");

  client.handleAgentMessage({
    method: "session/update",
    params: {
      sessionId: "session-1",
      update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: " hidden" } },
    },
  });
  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.body]), [["user", "two"], ["thinking", ""]]);

  client.setActiveWorkspace("niri:workspace:1", "1");
  assert.deepEqual(latestSnapshot(messages).map(entry => entry.body), ["one", "answer one hidden"]);
});

test("running prompts show a transient thinking entry after the user message", () => {
  const { client, messages, writes } = makeClient();

  client.initialize();
  respondTo(writes, client, "initialize", {});
  respondTo(writes, client, "session/new", { sessionId: "session-1" });
  client.queuePrompt("hello");

  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.body]), [["user", "hello"], ["thinking", ""]]);

  client.handleAgentMessage({
    method: "session/update",
    params: {
      sessionId: "session-1",
      update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "hi" } },
    },
  });

  assert.deepEqual(latestSnapshot(messages).map(entry => [entry.kind, entry.body]), [["user", "hello"], ["assistant", "hi"]]);
});

test("/clear closes and recreates only the active workspace session", () => {
  const { client, messages, writes } = makeClient();

  client.initialize();
  respondTo(writes, client, "initialize", {});
  respondTo(writes, client, "session/new", { sessionId: "session-1" });
  client.queuePrompt("hello");
  client.queuePrompt("/clear");

  assert.deepEqual(latestSnapshot(messages), []);
  assert.equal(writes.find(write => write.method === "session/close").params.sessionId, "session-1");
  assert.equal(writes.at(-1).method, "session/new");
});

test("available command updates feed slash command UI metadata", () => {
  const { client, messages, writes } = makeClient();

  client.initialize();
  respondTo(writes, client, "initialize", {});
  respondTo(writes, client, "session/new", { sessionId: "session-1" });
  client.handleAgentMessage({
    method: "session/update",
    params: {
      sessionId: "session-1",
      update: {
        sessionUpdate: "available_commands_update",
        availableCommands: [{ name: "explain", description: "Explain context" }],
      },
    },
  });

  const workspaceUpdate = messages.filter(message => message.type === "workspace").at(-1);
  assert.deepEqual(workspaceUpdate.commands.map(command => command.name), ["clear", "new", "cancel", "help", "explain"]);
});
