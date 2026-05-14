import QtQuick
import QtWebEngine

Item {
  id: root

  property var shell
  property bool webReady: false

  visible: !shell.sidebarCollapsed && !shell.agentPaneCollapsed
  width: shell.agentPaneWidth
  height: parent ? parent.height : 0

  function cssColor(colorValue) {
    const value = String(colorValue || "#000000");
    if (/^#[0-9a-fA-F]{8}$/.test(value)) {
      return "#" + value.substring(3, 9) + value.substring(1, 3);
    }
    return value;
  }

  function statePayload() {
    return {
      status: root.shell.agentStatus,
      events: root.shell.agentEvents || [],
      commands: root.shell.allAgentCommands(),
      references: root.shell.referenceItems(""),
      workspaceTitle: root.shell.activeWorkspaceLabel,
      theme: {
        primary: cssColor(root.shell.mPrimary),
        onPrimary: cssColor(root.shell.mOnPrimary),
        secondary: cssColor(root.shell.mSecondary),
        tertiary: cssColor(root.shell.mTertiary),
        error: cssColor(root.shell.mError),
        onError: cssColor(root.shell.mOnError),
        surface: cssColor(root.shell.mSurface),
        onSurface: cssColor(root.shell.mOnSurface),
        surfaceVariant: cssColor(root.shell.mSurfaceVariant),
        onSurfaceVariant: cssColor(root.shell.mOnSurfaceVariant),
        outline: cssColor(root.shell.mOutline),
        capsule: cssColor(root.shell.capsuleColor),
        capsuleHover: cssColor(root.shell.capsuleHoverColor)
      }
    };
  }

  function pushState() {
    if (!webReady) {
      return;
    }
    const script = "window.ticAgent && window.ticAgent.receive(" + JSON.stringify(statePayload()) + ");";
    agentWebView.runJavaScript(script);
  }

  function handleWebMessage(rawMessage) {
    let message = null;
    try {
      message = JSON.parse(rawMessage);
    } catch (error) {
      return;
    }

    if (message.type === "ready" || message.type === "requestState") {
      webReady = true;
      pushState();
    } else if (message.type === "prompt") {
      root.shell.sendAgentPrompt(String(message.text || ""));
    } else if (message.type === "control") {
      root.shell.sendAgentControl(String(message.action || ""));
    }
  }

  WebEngineView {
    id: agentWebView

    anchors.fill: parent
    url: "file://" + root.shell.ticShellRoot + "/shell/noctalia/Modules/TicWorkspace/Web/index.html?v=8"
    backgroundColor: "transparent"

    onLoadingChanged: loadRequest => {
      if (loadRequest.status === WebEngineView.LoadSucceededStatus) {
        root.webReady = true;
        root.pushState();
      }
    }

    onJavaScriptConsoleMessage: (level, message, lineNumber, sourceID) => {
      const prefix = "__tic_agent__:";
      if (message.indexOf(prefix) === 0) {
        root.handleWebMessage(message.substring(prefix.length));
      } else {
        console.log("AgentWeb", sourceID + ":" + lineNumber, message);
      }
    }
  }

  Timer {
    id: syncTimer

    interval: 160
    repeat: true
    running: root.visible
    onTriggered: root.pushState()
  }

  onVisibleChanged: {
    if (visible) {
      pushState();
    }
  }
}
