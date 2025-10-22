{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk.url = "github:nix-community/naersk/master";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    utils.url = "github:numtide/flake-utils";
    utils.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        xDeps = with pkgs; [
          libxkbcommon
          libGL

          # x11 lib
          xorg.libX11
          xorg.libXrandr
          xorg.libXcursor
          xorg.libXi

          # wayland
          wayland
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

            # Install desktop entry
            mkdir -p $out/share/applications
            cp ${./resources/malakal.desktop} $out/share/applications/
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
