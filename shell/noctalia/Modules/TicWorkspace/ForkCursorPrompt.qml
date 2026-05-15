import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Modules.TicWorkspace.Services as TicServices

Item {
  id: root

  property var screenDetector
  property bool promptOpen: false
  property string activeScreenName: ""
  property string activeNamespace: ""
  property string promptText: ""

  function namespaceForScreen(screen) {
    return "tic-fork-cursor-prompt-" + (screen?.name || "unknown");
  }

  function openPrompt() {
    if (!screenDetector) {
      return;
    }

    screenDetector.withCurrentScreen(function(screen) {
      activeScreenName = screen?.name || "";
      activeNamespace = namespaceForScreen(screen);
      promptText = "";
      promptOpen = true;
      Qt.callLater(registerOverlay);
    }, true);
  }

  function closePrompt() {
    unregisterOverlay();
    promptOpen = false;
    promptText = "";
  }

  function submitPrompt() {
    const text = promptText.trim();
    if (text.length > 0) {
      TicServices.ForkCursorService.submitPrompt(text);
    }
    closePrompt();
  }

  function registerOverlay() {
    if (!activeNamespace) {
      return;
    }
    overlayProcess.command = [
      "niri", "msg", "register-cursor-overlay",
      "--overlay-id", "tic-fork-cursor-prompt",
      "--layer-namespace", activeNamespace,
      "--anchor-hardware-pointer",
      "--side", "right",
      "--align", "center",
      "--edge-padding", "8",
      "--interactive",
      "--keyboard-focus",
      "--replace-existing"
    ];
    overlayProcess.running = true;
  }

  function unregisterOverlay() {
    if (!activeNamespace) {
      return;
    }
    cleanupProcess.command = [
      "niri", "msg", "unregister-cursor-overlay",
      "--overlay-id", "tic-fork-cursor-prompt"
    ];
    cleanupProcess.running = true;
  }

  Process {
    id: overlayProcess
    running: false
  }

  Connections {
    target: TicServices.ForkCursorService

    function onPromptRequested() {
      root.openPrompt();
    }
  }

  Process {
    id: cleanupProcess
    running: false
  }

  Variants {
    model: Quickshell.screens

    PanelWindow {
      id: promptWindow

      property var modelScreen: modelData

      screen: modelScreen
      visible: root.promptOpen && root.activeScreenName === (modelScreen?.name || "")
      color: "transparent"
      implicitWidth: 320
      implicitHeight: 54
      focusable: true

      WlrLayershell.namespace: root.namespaceForScreen(modelScreen)
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.exclusionMode: ExclusionMode.Ignore
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

      anchors {
        top: true
        left: true
      }

      Rectangle {
        anchors.fill: parent
        radius: 8
        color: Qt.alpha(Color.mSurface, 0.96)
        border.color: Color.mPrimary
        border.width: 1

        TextInput {
          id: promptInput

          anchors {
            fill: parent
            leftMargin: 14
            rightMargin: 14
          }
          verticalAlignment: TextInput.AlignVCenter
          color: Color.mOnSurface
          selectionColor: Qt.alpha(Color.mPrimary, 0.35)
          selectedTextColor: Color.mOnSurface
          font.pixelSize: 14
          clip: true
          focus: promptWindow.visible
          text: root.promptText
          onTextChanged: root.promptText = text
          onAccepted: root.submitPrompt()

          Keys.onEscapePressed: root.closePrompt()
        }
      }

      onVisibleChanged: {
        if (visible) {
          Qt.callLater(function() {
            promptInput.forceActiveFocus();
          });
        }
      }
    }
  }
}
