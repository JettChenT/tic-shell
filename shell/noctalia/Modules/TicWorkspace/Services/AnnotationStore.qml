import QtQuick

Item {
  id: root

  property var annotations: ({})
  property bool ready: true

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
  }
}
