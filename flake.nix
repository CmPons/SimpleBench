{
  description = "Dev Shell for working on The Forge";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    semantic-release-cargo.url = "github:CmPons/nix-semantic-release-cargo";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      semantic-release-cargo,
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
      in
      {
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
