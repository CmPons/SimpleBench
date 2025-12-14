{
  description = "SimpleBench - A minimalist microbenchmarking framework for Rust";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    semantic-release-cargo.url = "github:CmPons/nix-semantic-release-cargo";
    cargo-simplebench-repo.url = "github:CmPons/SimpleBench";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      semantic-release-cargo,
      cargo-simplebench-repo,
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

        rustPlatform = pkgs.rustPlatform;

        cargo-simplebench =
          let
            cargoToml = builtins.fromTOML (builtins.readFile "${cargo-simplebench-repo}/Cargo.toml");
          in
          rustPlatform.buildRustPackage {
            pname = "cargo-simplebench";
            version = "v${cargoToml.workspace.package.version}";
            src = "${cargo-simplebench-repo}";
            cargoLock = {
              lockFile = "${cargo-simplebench-repo}/Cargo.lock";
            };

            # Some tests require access to the default network device to determine the MAC address
            # This is hashed and then used to store the benchmark data per machine
            doCheck = false;

            # We are in a workspace specify just cargo-simplebench
            cargoBuildFlags = [
              "-p"
              "cargo-simplebench"
            ];
            cargoTestFlags = [
              "-p"
              "cargo-simplebench"
            ];

            meta = with pkgs.lib; {
              description = "A minimalist microbenchmarking framework for Rust with clear regression detection";
              homepage = "https://github.com/CmPons/SimpleBench";
              license = licenses.mit;
            };
          };
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
