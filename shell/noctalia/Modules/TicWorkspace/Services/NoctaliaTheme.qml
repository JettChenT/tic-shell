import QtQuick
import Quickshell
import Quickshell.Io

Item {
  id: root

  readonly property string configDir: (Quickshell.env("NOCTALIA_CONFIG_DIR") || ((Quickshell.env("XDG_CONFIG_HOME") || (Quickshell.env("HOME") + "/.config")) + "/noctalia/"))
  readonly property string colorsFile: configDir + (configDir.endsWith("/") ? "" : "/") + "colors.json"

  property color mPrimary: "#fff59b"
  property color mOnPrimary: "#0e0e43"
  property color mSecondary: "#a9aefe"
  property color mOnSecondary: "#0e0e43"
  property color mTertiary: "#9BFECE"
  property color mOnTertiary: "#0e0e43"
  property color mError: "#FD4663"
  property color mOnError: "#0e0e43"
  property color mSurface: "#070722"
  property color mOnSurface: "#f3edf7"
  property color mSurfaceVariant: "#11112d"
  property color mOnSurfaceVariant: "#7c80b4"
  property color mOutline: "#21215F"
  property color mHover: "#9BFECE"
  property color mOnHover: "#0e0e43"

  function applyAdapter() {
    mPrimary = colorsAdapter.mPrimary || mPrimary;
    mOnPrimary = colorsAdapter.mOnPrimary || mOnPrimary;
    mSecondary = colorsAdapter.mSecondary || mSecondary;
    mOnSecondary = colorsAdapter.mOnSecondary || mOnSecondary;
    mTertiary = colorsAdapter.mTertiary || mTertiary;
    mOnTertiary = colorsAdapter.mOnTertiary || mOnTertiary;
    mError = colorsAdapter.mError || mError;
    mOnError = colorsAdapter.mOnError || mOnError;
    mSurface = colorsAdapter.mSurface || mSurface;
    mOnSurface = colorsAdapter.mOnSurface || mOnSurface;
    mSurfaceVariant = colorsAdapter.mSurfaceVariant || mSurfaceVariant;
    mOnSurfaceVariant = colorsAdapter.mOnSurfaceVariant || mOnSurfaceVariant;
    mOutline = colorsAdapter.mOutline || mOutline;
    mHover = colorsAdapter.mHover || mHover;
    mOnHover = colorsAdapter.mOnHover || mOnHover;
  }

  FileView {
    id: colorsView

    path: root.colorsFile
    watchChanges: true
    printErrors: false

    adapter: JsonAdapter {
      id: colorsAdapter

      property color mPrimary: root.mPrimary
      property color mOnPrimary: root.mOnPrimary
      property color mSecondary: root.mSecondary
      property color mOnSecondary: root.mOnSecondary
      property color mTertiary: root.mTertiary
      property color mOnTertiary: root.mOnTertiary
      property color mError: root.mError
      property color mOnError: root.mOnError
      property color mSurface: root.mSurface
      property color mOnSurface: root.mOnSurface
      property color mSurfaceVariant: root.mSurfaceVariant
      property color mOnSurfaceVariant: root.mOnSurfaceVariant
      property color mOutline: root.mOutline
      property color mHover: root.mHover
      property color mOnHover: root.mOnHover
    }

    onLoaded: root.applyAdapter()
    onAdapterUpdated: root.applyAdapter()
    onLoadFailed: function(_error) {}
  }
}
