import React, { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { Streamdown } from "streamdown";

const emptyState = {
  status: "starting",
  events: [],
  commands: [],
  references: [],
  workspaceTitle: "Workspace",
  theme: {},
};

function post(message) {
  console.log("__tic_agent__:" + JSON.stringify(message));
}

function stableKey(value) {
  return JSON.stringify(value ?? null);
}

function imageSource(source) {
  const text = String(source || "");
  if (!text || /^(data|file|https?|qrc|blob):/.test(text)) {
    return text;
  }
  return text.startsWith("/") ? "file://" + encodeURI(text) : text;
}

function browserImageSource(source) {
  const text = String(source || "");
  if (!text) {
    return "";
  }
  if (/^(data|file|https?|blob):/.test(text) || text.startsWith("/")) {
    return imageSource(text);
  }
  return "";
}

function appInitial(reference) {
  const appId = String(reference?.appId || reference?.detail || reference?.label || "?");
  const normalized = appId.replace(/^com\./, "").replace(/^org\./, "");
  const parts = normalized.split(/[.\-_ ]+/).filter(part => part.length > 0);
  const token = parts.length > 0 ? parts[parts.length - 1] : normalized;
  return token.substring(0, 1).toUpperCase() || "?";
}

function unique(values) {
  return [...new Set(values.filter(Boolean))];
}

function iconNameCandidates(reference) {
  const appId = String(reference?.appId || reference?.detail || "");
  const normalized = appId.replace(/^com\./, "").replace(/^org\./, "");
  const parts = normalized.split(/[.\-_ ]+/).filter(part => part.length > 0);
  return unique([
    reference?.iconName,
    appId,
    appId.toLowerCase(),
    normalized,
    normalized.toLowerCase(),
    parts[parts.length - 1]?.toLowerCase(),
  ]);
}

function hicolorIconCandidates(reference) {
  const roots = [
    "/run/current-system/sw/share/icons/hicolor",
    "/etc/profiles/per-user/jettc/share/icons/hicolor",
  ];
  const sizes = ["32x32", "48x48", "64x64", "128x128", "256x256", "scalable"];
  const extensions = ["png", "svg"];
  const result = [];
  for (const name of iconNameCandidates(reference)) {
    for (const root of roots) {
      for (const size of sizes) {
        for (const extension of extensions) {
          result.push(`file://${root}/${size}/apps/${encodeURIComponent(name)}.${extension}`);
        }
      }
    }
  }
  return unique(result);
}

function iconCandidates(reference) {
  return unique([
    browserImageSource(reference?.iconPath),
    ...hicolorIconCandidates(reference),
  ]);
}

function applyTheme(theme) {
  if (!theme) {
    return;
  }
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme)) {
    const cssName = "--" + key.replace(/[A-Z]/g, match => "-" + match.toLowerCase());
    root.style.setProperty(cssName, value);
  }
}

function App() {
  const [agentState, setAgentState] = useState(emptyState);
  const keysRef = useRef({ state: "", theme: "" });

  useEffect(() => {
    window.ticAgent = {
      receive(nextState) {
        const next = nextState || emptyState;
        const nextKey = stableKey({
          status: next.status,
          events: next.events,
          commands: next.commands,
          references: next.references,
          workspaceTitle: next.workspaceTitle,
        });
        const nextThemeKey = stableKey(next.theme || {});

        if (nextThemeKey !== keysRef.current.theme) {
          keysRef.current.theme = nextThemeKey;
          applyTheme(next.theme);
        }
        if (nextKey !== keysRef.current.state) {
          keysRef.current.state = nextKey;
          setAgentState({ ...emptyState, ...next });
        }
      },
    };
    post({ type: "ready" });
  }, []);

  const sendControl = useCallback(action => post({ type: "control", action }), []);
  const sendPrompt = useCallback(text => post({ type: "prompt", text }), []);

  return (
    <div className="agent-chat">
      <Header status={agentState.status} onControl={sendControl} />
      <Transcript events={agentState.events} references={agentState.references} />
      <Composer commands={agentState.commands} references={agentState.references} onSubmit={sendPrompt} />
    </div>
  );
}

function Header({ status, onControl }) {
  const statusVisible = status === "error" || status === "stopped";
  return (
    <header className="pane-header">
      <div className="title-stack">
        <div className="pane-title">Agent</div>
        <div className={"status-line" + (statusVisible ? " visible" : "")}>{statusVisible ? status : ""}</div>
      </div>
      <div className="header-actions">
        <button className="icon-button" title="New session" onClick={() => onControl("new")}>+</button>
        <button className="icon-button" title="Clear session" onClick={() => onControl("clear")}>C</button>
        <button className="icon-button danger" title="Cancel turn" onClick={() => onControl("cancel")}>x</button>
      </div>
    </header>
  );
}

const Transcript = memo(function Transcript({ events, references }) {
  const ref = useRef(null);
  const wasNearBottom = useRef(true);

  useEffect(() => {
    const node = ref.current;
    if (!node || !wasNearBottom.current) {
      return;
    }
    requestAnimationFrame(() => {
      node.scrollTop = node.scrollHeight;
    });
  }, [events]);

  return (
    <section
      ref={ref}
      className="transcript"
      aria-live="polite"
      onScroll={event => {
        const node = event.currentTarget;
        wasNearBottom.current = node.scrollTop + node.clientHeight >= node.scrollHeight - 32;
      }}
    >
      {(events || []).map(event => (
        <EventCard key={event.id || `${event.kind}:${event.time}:${event.body}`} event={event} references={references} />
      ))}
    </section>
  );
});

const EventCard = memo(function EventCard({ event, references }) {
  const kind = event.kind || "system";
  const isTool = kind === "tool";
  const hasHeader = isTool || kind === "permission" || kind === "thinking" || kind === "thought";

  return (
    <article className={`event ${kind}`}>
      <div className="event-inner">
        {hasHeader ? <EventHeader event={event} /> : null}
        {isTool ? <ToolBody event={event} /> : <MessageBody event={event} references={references} />}
      </div>
    </article>
  );
});

function EventHeader({ event }) {
  const [expanded, setExpanded] = useToolExpansion(event.id);
  const { status, output } = toolParts(event);
  return (
    <div className="event-header">
      {event.kind === "thinking" ? <div className="spinner" /> : null}
      {event.kind === "tool" ? <ToolIcon event={event} /> : null}
      <span className="event-title">{event.title || "Agent"}</span>
      {event.kind === "tool" && status ? <span className="tool-status">{status}</span> : null}
      {event.kind === "tool" && output ? (
        <button className="tool-disclosure" type="button" onClick={() => setExpanded(!expanded)}>
          {expanded ? "-" : "+"}
        </button>
      ) : null}
      {event.kind !== "thinking" ? <span className="event-time">{event.time || ""}</span> : null}
    </div>
  );
}

const expandedToolIds = new Set();

function useToolExpansion(id) {
  const [expanded, setExpandedState] = useState(expandedToolIds.has(id));
  const setExpanded = useCallback(next => {
    if (next) {
      expandedToolIds.add(id);
    } else {
      expandedToolIds.delete(id);
    }
    setExpandedState(next);
  }, [id]);
  return [expanded, setExpanded];
}

function MessageBody({ event, references }) {
  if (event.kind === "thinking" && !String(event.body || "").length) {
    return null;
  }
  const markdownKinds = new Set(["assistant", "thought", "system", "stderr", "permission"]);
  return (
    <div className="body">
      {referenceSegmentsInText(event.body || "", references).map((segment, index) => (
        segment.kind === "reference"
          ? <ReferenceBadge key={index} reference={segment.reference} />
          : markdownKinds.has(event.kind)
            ? <MarkdownText key={index} text={segment.text || ""} streaming={event.kind === "assistant" || event.kind === "thought"} />
            : <span key={index} className="body-text">{segment.text || ""}</span>
      ))}
    </div>
  );
}

const MarkdownText = memo(function MarkdownText({ text, streaming }) {
  return (
    <div className="markdown-body">
      <Streamdown isAnimating={streaming}>{text}</Streamdown>
    </div>
  );
});

function ToolBody({ event }) {
  const [expanded] = useToolExpansion(event.id);
  const metadata = event.metadata || {};
  const image = metadata.image || {};
  const { output } = toolParts(event);

  if (image.source) {
    return (
      <div className="tool-body">
        <div className="tool-image-frame">
          <img src={imageSource(image.source)} alt={event.title || "tool image"} />
        </div>
      </div>
    );
  }

  if (!output) {
    return <div className="tool-body" />;
  }

  const text = expanded ? output : previewText(output);
  const lineCount = output.split("\n").length;
  return (
    <div className="tool-body">
      <div className={"tool-output" + (expanded ? " expanded" : "")}>{text}</div>
      {!expanded && lineCount > 3 ? <div className="tool-lines">{lineCount} output lines</div> : null}
    </div>
  );
}

function ToolIcon({ event }) {
  const name = String((event.metadata || {}).toolName || "");
  const paths = {
    click: ["M3 3l7.07 16.97 2.51-7.39 7.39-2.51L3 3z", "M13 13l6 6"],
    "type-text": ["M17 6.1H3", "M21 12.1H3", "M15.1 18H3"],
    scroll: ["M10 6h4", "M12 2v20", "M8 18l4 4 4-4", "M8 6l4-4 4 4"],
    screenshot: ["M3 7h4l2-3h6l2 3h4v13H3z", "M12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8z"],
    terminal: ["M4 17l6-6-6-6", "M12 19h8"],
    window: ["M3 5h18v14H3z", "M3 9h18"],
  };
  const selected = name === "click" ? paths.click
    : name === "type-text" ? paths["type-text"]
    : name === "scroll" ? paths.scroll
    : (name === "describe-workspace" || name === "view-window" || name === "screenshot-window") ? paths.screenshot
    : (event.metadata || {}).isCua ? paths.window
    : paths.terminal;

  return (
    <svg className="tool-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      {selected.map(d => <path key={d} d={d} />)}
    </svg>
  );
}

function toolParts(event) {
  const lines = String(event.body || "").split("\n");
  return {
    status: lines[0] || "",
    output: lines.slice(1).join("\n").trim(),
  };
}

function previewText(text) {
  const lines = String(text || "").split("\n");
  const preview = lines.slice(0, 3).join("\n").trim();
  return lines.length > 3 ? preview + "\n..." : preview;
}

function referenceSegmentsInText(text, references) {
  const source = String(text || "");
  const regex = /\[@([^\]]+)\]\(tic:\/\/(workspace|window)\/([^)]+)\)/g;
  const result = [];
  let cursor = 0;
  let match;
  while ((match = regex.exec(source)) !== null) {
    if (match.index > cursor) {
      result.push({ kind: "text", text: source.substring(cursor, match.index) });
    }
    const existing = (references || []).find(ref => ref.type === match[2] && String(ref.id) === String(match[3]));
    result.push({
      kind: "reference",
      reference: existing || {
        type: match[2],
        id: Number(match[3]),
        label: match[1],
        detail: match[2],
        iconPath: "",
      },
    });
    cursor = regex.lastIndex;
  }
  if (cursor < source.length) {
    result.push({ kind: "text", text: source.substring(cursor) });
  }
  return result;
}

function ReferenceBadge({ reference }) {
  return (
    <span className="reference-badge" contentEditable={false} data-reference={JSON.stringify(reference || {})}>
      <ReferenceIcon reference={reference} />
      <span className="badge-label">{reference?.label || ""}</span>
    </span>
  );
}

function ReferenceIcon({ reference }) {
  const candidates = reference?.type === "window" ? iconCandidates(reference) : [];
  if (candidates.length > 0) {
    return <ReferenceIconImage reference={reference} candidates={candidates} />;
  }
  if (reference?.type === "window") {
    return <span className="badge-icon badge-icon-fallback">{appInitial(reference)}</span>;
  }
  const paths = reference?.type === "window"
    ? ["M3 5h18v14H3z", "M3 9h18"]
    : ["M3 5h7v7H3z", "M14 5h7v7h-7z", "M3 16h7v3H3z", "M14 16h7v3h-7z"];
  return (
    <svg className="badge-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      {paths.map(d => <path key={d} d={d} />)}
    </svg>
  );
}

function ReferenceIconImage({ reference, candidates }) {
  const [index, setIndex] = useState(0);
  if (index >= candidates.length) {
    return <span className="badge-icon badge-icon-fallback">{appInitial(reference)}</span>;
  }
  return (
    <img
      className="badge-icon"
      src={candidates[index]}
      alt=""
      onError={() => setIndex(value => value + 1)}
    />
  );
}

function Composer({ commands, references, onSubmit }) {
  const composerRef = useRef(null);
  const [revision, setRevision] = useState(0);
  const [slashIndex, setSlashIndex] = useState(0);
  const [referenceIndex, setReferenceIndex] = useState(0);
  const plainText = useCallback(() => plainComposerText(composerRef.current), []);
  const beforeCaret = useCallback(() => currentTextBeforeCaret(composerRef.current), []);
  const slashQuery = useMemo(() => {
    const text = plainText();
    if (!text.startsWith("/") || text.includes(" ") || text.includes("\n")) {
      return null;
    }
    return text.substring(1).toLowerCase();
  }, [plainText, revision]);
  const referenceQueryValue = useMemo(() => {
    if (plainText().startsWith("/")) {
      return null;
    }
    const match = beforeCaret().match(/(^|\s)@([^\s@]*)$/);
    return match ? match[2] || "" : null;
  }, [beforeCaret, plainText, revision]);
  const filteredCommands = useMemo(() => {
    if (slashQuery === null) {
      return [];
    }
    return (commands || []).filter(command => {
      const name = String(command.name || "").toLowerCase();
      const description = String(command.description || "").toLowerCase();
      return slashQuery.length === 0 || name.includes(slashQuery) || description.includes(slashQuery);
    });
  }, [commands, slashQuery]);
  const filteredReferences = useMemo(() => {
    if (referenceQueryValue === null) {
      return [];
    }
    const q = referenceQueryValue.toLowerCase();
    return (references || []).filter(ref => {
      const label = String(ref.label || "").toLowerCase();
      const detail = String(ref.detail || "").toLowerCase();
      return q.length === 0 || label.includes(q) || detail.includes(q) || String(ref.id).includes(q);
    });
  }, [references, referenceQueryValue]);

  const bump = useCallback(() => setRevision(value => value + 1), []);
  const submit = useCallback(() => {
    const text = serializeComposer(composerRef.current).trim();
    if (!text) {
      return;
    }
    onSubmit(text);
    composerRef.current.replaceChildren();
    bump();
  }, [bump, onSubmit]);
  const completeCommand = useCallback(command => {
    composerRef.current.textContent = "/" + command.name + " ";
    focusEnd(composerRef.current);
    bump();
  }, [bump]);
  const completeReference = useCallback(reference => {
    insertReferenceBadge(composerRef.current, reference);
    bump();
  }, [bump]);

  useEffect(() => {
    setSlashIndex(index => clamp(index, 0, filteredCommands.length - 1));
  }, [filteredCommands.length]);
  useEffect(() => {
    setReferenceIndex(index => clamp(index, 0, filteredReferences.length - 1));
  }, [filteredReferences.length]);

  return (
    <section className="composer-shell">
      {filteredCommands.length > 0 && filteredReferences.length === 0 ? (
        <div className="popup slash-popup">
          {filteredCommands.map((command, index) => (
            <div
              key={command.name}
              className={"popup-row" + (index === slashIndex ? " selected" : "")}
              onMouseEnter={() => setSlashIndex(index)}
              onMouseDown={event => {
                event.preventDefault();
                completeCommand(command);
              }}
            >
              <span className="slash-name">/{command.name}</span>
              <span className="slash-description">{command.description || ""}</span>
            </div>
          ))}
        </div>
      ) : null}
      {filteredReferences.length > 0 ? (
        <div className="popup reference-popup">
          {filteredReferences.map((reference, index) => (
            <div
              key={`${reference.type}:${reference.id}`}
              className={"popup-row" + (index === referenceIndex ? " selected" : "")}
              onMouseEnter={() => setReferenceIndex(index)}
              onMouseDown={event => {
                event.preventDefault();
                completeReference(reference);
              }}
            >
              <ReferenceIcon reference={reference} />
              <div className="popup-main">
                <div className="popup-label">{reference.label || ""}</div>
                <div className="popup-detail">{reference.type === "window" ? (reference.detail || "window") : (reference.detail || "workspace")}</div>
              </div>
            </div>
          ))}
        </div>
      ) : null}
      <div
        ref={composerRef}
        className="composer-input"
        contentEditable
        spellCheck
        role="textbox"
        aria-multiline="true"
        data-placeholder="ask Agent"
        onInput={bump}
        onClick={bump}
        onKeyUp={bump}
        onKeyDown={event => {
          if (event.key === "ArrowDown" && filteredReferences.length > 0) {
            setReferenceIndex(index => Math.min(filteredReferences.length - 1, index + 1));
            event.preventDefault();
          } else if (event.key === "ArrowUp" && filteredReferences.length > 0) {
            setReferenceIndex(index => Math.max(0, index - 1));
            event.preventDefault();
          } else if (event.key === "ArrowDown" && filteredCommands.length > 0) {
            setSlashIndex(index => Math.min(filteredCommands.length - 1, index + 1));
            event.preventDefault();
          } else if (event.key === "ArrowUp" && filteredCommands.length > 0) {
            setSlashIndex(index => Math.max(0, index - 1));
            event.preventDefault();
          } else if ((event.key === "Tab" || event.key === "Enter") && filteredReferences.length > 0 && !event.shiftKey) {
            completeReference(filteredReferences[referenceIndex]);
            event.preventDefault();
          } else if ((event.key === "Tab" || event.key === "Enter") && filteredCommands.length > 0 && !event.shiftKey) {
            completeCommand(filteredCommands[slashIndex]);
            event.preventDefault();
          } else if (event.key === "Enter" && event.shiftKey) {
            document.execCommand("insertLineBreak");
            event.preventDefault();
            bump();
          } else if (event.key === "Enter") {
            submit();
            event.preventDefault();
          } else if (event.key === "Escape") {
            setSlashIndex(0);
            setReferenceIndex(0);
            composerRef.current.blur();
            event.preventDefault();
          }
        }}
      />
      <footer className="composer-footer">
        <span>All actions allowed</span>
        <button className="send-button" onClick={submit}>Send</button>
      </footer>
    </section>
  );
}

function clamp(value, min, max) {
  if (max < min) {
    return 0;
  }
  return Math.max(min, Math.min(value, max));
}

function serializeComposer(root) {
  let result = "";
  for (const node of root.childNodes) {
    result += serializeNode(node);
  }
  return result.replace(/\u00a0/g, " ");
}

function serializeNode(node) {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent || "";
  }
  if (node.nodeType !== Node.ELEMENT_NODE) {
    return "";
  }
  if (node.classList.contains("reference-badge")) {
    const ref = JSON.parse(node.dataset.reference || "{}");
    const safeLabel = String(ref.label || ref.type || "reference").replace(/[\[\]\n\r]/g, " ").trim();
    return "[@" + safeLabel + "](tic://" + ref.type + "/" + ref.id + ")";
  }
  if (node.tagName === "BR") {
    return "\n";
  }
  let text = "";
  for (const child of node.childNodes) {
    text += serializeNode(child);
  }
  return text;
}

function plainComposerText(root) {
  if (!root) {
    return "";
  }
  let result = "";
  for (const node of root.childNodes) {
    if (node.nodeType === Node.ELEMENT_NODE && node.classList.contains("reference-badge")) {
      const ref = JSON.parse(node.dataset.reference || "{}");
      result += "@" + (ref.label || "");
    } else {
      result += node.textContent || "";
    }
  }
  return result;
}

function currentTextBeforeCaret(root) {
  const selection = window.getSelection();
  if (!root || !selection || selection.rangeCount === 0 || !root.contains(selection.anchorNode)) {
    return "";
  }
  const range = selection.getRangeAt(0).cloneRange();
  range.selectNodeContents(root);
  range.setEnd(selection.anchorNode, selection.anchorOffset);
  const container = document.createElement("div");
  container.append(range.cloneContents());
  return plainComposerText(container);
}

function insertReferenceBadge(root, reference) {
  const selection = window.getSelection();
  if (!root || !selection || selection.rangeCount === 0) {
    return;
  }

  if (selection.anchorNode?.nodeType === Node.TEXT_NODE) {
    const textNode = selection.anchorNode;
    const offset = selection.anchorOffset;
    const before = textNode.textContent.substring(0, offset);
    const after = textNode.textContent.substring(offset).replace(/^\s+/, "");
    const match = before.match(/(^|\s)@([^\s@]*)$/);
    if (match) {
      const parent = textNode.parentNode;
      const prefix = before.substring(0, before.length - match[0].length) + match[1];
      const badge = badgeElement(reference);
      const spacer = document.createTextNode(" ");
      textNode.textContent = prefix;
      parent.insertBefore(badge, textNode.nextSibling);
      parent.insertBefore(spacer, badge.nextSibling);
      if (after.length > 0) {
        parent.insertBefore(document.createTextNode(after), spacer.nextSibling);
      }
      const range = document.createRange();
      range.setStart(spacer, 1);
      range.collapse(true);
      selection.removeAllRanges();
      selection.addRange(range);
      root.focus();
      return;
    }
  }

  const range = selection.getRangeAt(0);
  range.insertNode(document.createTextNode(" "));
  range.insertNode(badgeElement(reference));
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
  root.focus();
}

function badgeElement(reference) {
  const badge = document.createElement("span");
  badge.className = "reference-badge";
  badge.contentEditable = "false";
  badge.dataset.reference = JSON.stringify(reference || {});
  const candidates = reference?.type === "window" ? iconCandidates(reference) : [];
  if (candidates.length > 0) {
    const img = document.createElement("img");
    img.className = "badge-icon";
    img.alt = "";
    let index = 0;
    img.src = candidates[index];
    img.onerror = () => {
      index += 1;
      if (index < candidates.length) {
        img.src = candidates[index];
      } else {
        img.replaceWith(fallbackIconElement(reference));
      }
    };
    badge.append(img);
  } else {
    badge.append(fallbackIconElement(reference));
  }
  const label = document.createElement("span");
  label.className = "badge-label";
  label.textContent = reference?.label || "";
  badge.append(label);
  return badge;
}

function fallbackIconElement(reference) {
  const icon = document.createElement("span");
  icon.className = "badge-icon badge-icon-fallback";
  icon.textContent = reference?.type === "window" ? appInitial(reference) : "▦";
  return icon;
}

function focusEnd(root) {
  root.focus();
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  const selection = window.getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
}

createRoot(document.getElementById("agent-chat")).render(<App />);
