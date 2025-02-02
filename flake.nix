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
        defaultPackage = with pkgs; naersk-lib.buildPackage {
          nativeBuildInputs = [ makeWrapper ];
          buildInputs = xDeps;
          src = ./.;
          postInstall = ''
          wrapProgram "$out/bin/malakal" \
            --set LD_LIBRARY_PATH "${lib.makeLibraryPath xDeps}"
          '';
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
