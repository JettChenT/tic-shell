import QtQuick
import Quickshell.Io

Item {
  id: root

  property string stateFile: ""
  property var annotations: ({})
  property bool ready: false

  function workspaceKey(workspaceId) {
    return "niri:workspace:" + workspaceId;
  }

  function annotationFor(workspaceId) {
    const entry = annotations[workspaceKey(workspaceId)];
    return entry && entry.annotation ? entry.annotation : "";
  }

  function setAnnotation(workspaceId, annotation) {
    const key = workspaceKey(workspaceId);
    const next = Object.assign({}, annotations);
    const trimmed = annotation.trim();

    if (trimmed.length === 0) {
      delete next[key];
    } else {
      next[key] = {
        annotation: trimmed,
        updatedAt: new Date().toISOString()
      };
    }

    annotations = next;
    annotationAdapter.workspaces = annotations;
    annotationFile.writeAdapter();
  }

  FileView {
    id: annotationFile

    path: root.stateFile
    watchChanges: true
    printErrors: false

    adapter: JsonAdapter {
      id: annotationAdapter

      property var workspaces: ({})
    }

    onLoaded: {
      root.annotations = annotationAdapter.workspaces || {};
      root.ready = true;
    }

    onLoadFailed: function(error) {
      root.annotations = {};
      annotationAdapter.workspaces = root.annotations;
      root.ready = true;
    }
  }
}
