import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons

Item {
  id: root

  property ShellScreen screen
  property var forkSessions: []

  function cleanId(id) {
    return String(id || "unknown").replace(/[^A-Za-z0-9_-]/g, "-");
  }

  function namespaceFor(fork) {
    return "tic-fork-status-" + cleanId(fork?.id || "");
  }

  function overlayIdFor(fork) {
    return "tic-fork-status-" + cleanId(fork?.id || "");
  }

  function barPositionForScreen() {
    return Settings.getBarPositionForScreen(screen?.name);
  }

  function overlaySideForScreen() {
    return barPositionForScreen() === "right" ? "left" : "right";
  }

  function overlayGapForScreen() {
    return barPositionForScreen() === "right" ? "32" : "18";
  }

  Variants {
    model: root.forkSessions || []

    PanelWindow {
      id: statusWindow

      required property var modelData
      property var forkSession: modelData
      property string overlayId: root.overlayIdFor(forkSession)
      property string layerNamespace: root.namespaceFor(forkSession)
      readonly property bool shouldShowStatus: !!forkSession.cursorId && forkSession.status !== "prompting"

      screen: root.screen
      visible: shouldShowStatus
      color: "transparent"
      implicitWidth: 236
      implicitHeight: 46

      WlrLayershell.namespace: layerNamespace
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.exclusionMode: ExclusionMode.Ignore
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

      anchors {
        top: true
        left: true
      }

      function registerOverlay() {
        if (!statusWindow.visible || !forkSession.cursorId || !layerNamespace) {
          return;
        }
        overlayProcess.command = [
          "niri", "msg", "register-cursor-overlay",
          "--overlay-id", overlayId,
          "--layer-namespace", layerNamespace,
          "--anchor-virtual-cursor", forkSession.cursorId,
          "--side", root.overlaySideForScreen(),
          "--align", "start",
          "--gap", root.overlayGapForScreen(),
          "--edge-padding", "8",
          "--replace-existing"
        ];
        overlayProcess.running = true;
      }

      function unregisterOverlay() {
        if (!overlayId) {
          return;
        }
        cleanupProcess.command = [
          "niri", "msg", "unregister-cursor-overlay",
          "--overlay-id", overlayId
        ];
        cleanupProcess.running = true;
      }

      Rectangle {
        anchors.fill: parent
        radius: 8
        color: Qt.alpha(Color.mSurface, 0.94)
        border.color: forkSession.status === "error" ? Color.mError : Color.mPrimary
        border.width: 1

        Column {
          anchors.fill: parent
          anchors.leftMargin: 10
          anchors.rightMargin: 10
          anchors.topMargin: 6
          anchors.bottomMargin: 6
          spacing: 2

          Text {
            width: parent.width
            color: Color.mOnSurface
            font.pixelSize: 12
            font.weight: Font.DemiBold
            elide: Text.ElideRight
            text: forkSession.status === "prompting" ? "Fork cursor" : (forkSession.title || "Fork cursor")
          }

          Text {
            width: parent.width
            color: Color.mOnSurfaceVariant
            font.pixelSize: 11
            elide: Text.ElideRight
            text: forkSession.statusMessage || forkSession.status || ""
          }
        }
      }

      Process {
        id: overlayProcess
        running: false
      }

      Process {
        id: cleanupProcess
        running: false
      }

      Component.onCompleted: if (visible) registerOverlay()
      Component.onDestruction: unregisterOverlay()
      onVisibleChanged: {
        if (visible) {
          registerOverlay();
        } else {
          unregisterOverlay();
        }
      }
    }
  }
}
