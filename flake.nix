{
  description = "SimpleBench - A minimalist microbenchmarking framework for Rust";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    semantic-release-cargo.url = "github:CmPons/nix-semantic-release-cargo";
    crane.url = "github:ipetkov/crane";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      semantic-release-cargo,
      crane,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
        crm = semantic-release-cargo.packages.${system}.default;

        craneLib = crane.mkLib pkgs;

        # Common arguments for crane builds
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
        };

        # Build dependencies first (for caching)
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the cargo-simplebench binary
        cargo-simplebench = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          cargoExtraArgs = "-p cargo-simplebench";
        });
      in
      {
        packages = {
          inherit cargo-simplebench;
          default = cargo-simplebench;
        };

        devShell = pkgs.mkShell {

          packages = with pkgs; [
            claude-code
            cargo-bump
            cargo-cross
            cargo-machete
            cargo-nextest
            cargo-release
            gdb
            mold
            pkg-config
            podman
            rust-analyzer
            zip
            wine64
            semantic-release
            crm
          ];

          # For when podman and docker are installed
          CROSS_CONTAINER_ENGINE = "podman";

          shellHook = ''
            export PATH=$PATH:"$HOME/.cargo/bin/"
            NIX_ENFORCE_PURITY=0
            exec ${pkgs.zsh}/bin/zsh
          '';
        };
      }
    );
}
