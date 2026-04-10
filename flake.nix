{
  description = "jellyfin-tui";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    systems.url = "github:nix-systems/default";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      systems,
      rust-overlay,
      ...
    }:
    let
      package =
        let
          cargoToml = fromTOML (builtins.readFile ./Cargo.toml);
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
        in
        {
          lib,
          stdenv,
          rustPlatform,
          pkg-config,
          openssl,
          mpv,
          sqlite,
          writableTmpDirAsHomeHook,
        }:
        rustPlatform.buildRustPackage {
          inherit pname;
          inherit version;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
            ];
          };

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ pkg-config ];

          buildInputs = [
            openssl
            mpv
            sqlite
          ];

          nativeInstallCheckInputs = [ writableTmpDirAsHomeHook ];
          doInstallCheck = true;

          preInstallCheck = ''
            mkdir -p "$HOME/${
              if stdenv.buildPlatform.isDarwin then "Library/Application Support" else ".local/share"
            }"
          '';

          postInstall = lib.optionalString stdenv.hostPlatform.isLinux ''
            install -Dm644 src/extra/jellyfin-tui.desktop $out/share/applications/jellyfin-tui.desktop
          '';
        };

      inherit (nixpkgs) lib;

      forEachPkgs =
        f:
        lib.genAttrs (import systems) (
          system:
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
            toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          in
          f { inherit pkgs toolchain; }
        );

    in
    {
      packages = forEachPkgs (
        { pkgs, toolchain }:
        let
          customRustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };

          jellyfin-tui = pkgs.callPackage package { rustPlatform = customRustPlatform; };
        in
        {
          default = jellyfin-tui;

          inherit jellyfin-tui;

          debug = jellyfin-tui.overrideAttrs (
            newAttrs: oldAttrs: {

              pname = oldAttrs.pname + "-debug";

              cargoBuildType = "debug";
              cargoCheckType = newAttrs.cargoBuildType;

              dontStrip = true;
            }
          );
        }
      );

      devShells = forEachPkgs (
        { pkgs, toolchain }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              toolchain
              openssl
              mpv
              sqlite
              pkg-config
            ];
            env = {
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
              OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
            };
          };
        }
      );
    };
}
