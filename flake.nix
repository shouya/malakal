{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        xDeps = with pkgs; [
          # development lib
          xorg.libX11
          xorg.libXrandr
          xorg.libXcursor
          xorg.libXi
          libxkbcommon
          libGL
        ];
        naersk-lib = pkgs.callPackage naersk {};
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          buildInputs = xDeps;
          src = ./.;
        };
        devShell = with pkgs; mkShell {
          buildInputs = [
            cargo
            rustc
            rustfmt
            pre-commit
            rustPackages.clippy
            rust-analyzer
          ] ++ xDeps;

          LD_LIBRARY_PATH = lib.makeLibraryPath xDeps;
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
        };
      }
    );
}
