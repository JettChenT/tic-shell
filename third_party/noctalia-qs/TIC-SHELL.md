# tic-shell local fork

This directory vendors `noctalia-qs` so tic-shell can carry shell-runtime patches
in the same repository as the QML that depends on them.

Local changes:

- Build Qt WebEngine support by default.
- Add `qt6.qtwebengine` to the Nix package inputs.
- Link the launcher with `Qt::WebEngineQuick`.
- Initialize Qt WebEngine before constructing the Qt application.
- Preserve the real process `argc` when launching, which avoids WebEngine
  aborting on an empty argument list.
