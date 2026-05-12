{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    noctalia-qs = {
      url = "github:noctalia-dev/noctalia-qs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    tiri = {
      url = "git+https://github.com/JettChenT/tiri.git?ref=main";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, noctalia-qs, tiri, ... }:
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
          niri = tiri.packages.${pkgs.stdenv.hostPlatform.system}.niri;
          quickshell = noctalia-qs.packages.${pkgs.stdenv.hostPlatform.system}.default;
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
