{
  lib,
  mkShell,

  atk,
  bun,
  cairo,
  cargo,
  clang-tools,
  fontconfig,
  freetype,
  gdk-pixbuf,
  glib,
  glibc,
  grim,
  gtk3,
  just,
  libpulseaudio,
  libx11,
  libxkbcommon,
  libxcb,
  llvmPackages,
  niri,
  nixd,
  nixfmt,
  openssl,
  pango,
  pipewire,
  pkg-config,
  quickshell,
  rustc,
  rustfmt,
  systemd,
  vulkan-loader,
  wayland,
}:

mkShell rec {
  packages = [
    bun
    cargo
    clang-tools
    grim
    just
    niri
    nixd
    nixfmt
    quickshell
    rustc
    rustfmt
  ];

  nativeBuildInputs = [
    pkg-config
    llvmPackages.libclang
  ];

  buildInputs = [
    atk
    cairo
    fontconfig
    fontconfig.dev
    freetype
    gdk-pixbuf
    glib
    gtk3
    libpulseaudio
    libxkbcommon
    openssl
    pango
    pipewire
    systemd
    vulkan-loader
    wayland
    libx11
    libxcb
  ];

  env = {
    LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
    LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
    BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${llvmPackages.libclang.lib}/lib/clang/${lib.versions.major llvmPackages.libclang.version}/include -isystem ${glibc.dev}/include";
  };
}
