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
  property bool submitted: false

  function namespaceForScreen(screen) {
    return "tic-fork-cursor-prompt-" + (screen?.name || "unknown");
  }

  function barPositionForActiveScreen() {
    return Settings.getBarPositionForScreen(activeScreenName);
  }

  function overlaySideForActiveScreen() {
    return barPositionForActiveScreen() === "right" ? "left" : "right";
  }

  function overlayGapForActiveScreen() {
    return barPositionForActiveScreen() === "right" ? "32" : "18";
  }

  function openPrompt() {
    if (!screenDetector) {
      return;
    }

    screenDetector.withCurrentScreen(function(screen) {
      activeScreenName = screen?.name || "";
      activeNamespace = namespaceForScreen(screen);
      promptText = "";
      submitted = false;
      promptOpen = false;
      registerOverlay();
    }, true);
  }

  function closePrompt() {
    if (promptOpen && !submitted) {
      TicServices.ForkCursorService.cancelPrompt();
    }
    unregisterOverlay();
    promptOpen = false;
    promptText = "";
    submitted = false;
  }

  function submitPrompt() {
    const text = promptText.trim();
    if (text.length > 0) {
      submitted = true;
      TicServices.ForkCursorService.submitPrompt(text);
      closeAfterSubmitTimer.restart();
      return;
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
      "--side", overlaySideForActiveScreen(),
      "--align", "center",
      "--gap", overlayGapForActiveScreen(),
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

    onExited: function(exitCode) {
      if (exitCode === 0 && root.activeNamespace) {
        root.promptOpen = true;
        TicServices.ForkCursorService.notifyPromptOpened();
      } else {
        root.promptOpen = false;
        root.promptText = "";
      }
    }
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

  Timer {
    id: closeAfterSubmitTimer

    interval: 350
    repeat: false
    onTriggered: root.closePrompt()
  }

  Variants {
    model: Quickshell.screens

    PanelWindow {
      id: promptWindow

      required property var modelData
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
