pragma Singleton

import QtQuick
import Quickshell

Singleton {
  id: root

  signal promptRequested
  signal promptSubmitted(string prompt)

  function requestPrompt() {
    promptRequested();
  }

  function submitPrompt(prompt) {
    const trimmed = String(prompt || "").trim();
    if (trimmed.length === 0) {
      return;
    }
    promptSubmitted(trimmed);
  }
}
