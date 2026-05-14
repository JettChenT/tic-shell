{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    noctalia-qs = {
      url = "path:./third_party/noctalia-qs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, noctalia-qs, ... }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs supportedSystems (
          system:
          let
            pkgs = import nixpkgs { inherit system; };
          in
          f pkgs
        );
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/shell.nix {
          niri = pkgs.runCommandLocal "tiri-debug-niri" { } ''
            mkdir -p "$out/bin"
            cat > "$out/bin/niri" <<'EOF'
            #!${pkgs.runtimeShell}
            unset LD_LIBRARY_PATH
            exec /home/jettc/dev/tiri/target/debug/niri "$@"
            EOF
            chmod +x "$out/bin/niri"
          '';
          quickshell = noctalia-qs.packages.${pkgs.stdenv.hostPlatform.system}.default;
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
