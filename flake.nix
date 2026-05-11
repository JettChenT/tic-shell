{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { nixpkgs, ... }:
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
        default = pkgs.callPackage ./nix/shell.nix { };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
