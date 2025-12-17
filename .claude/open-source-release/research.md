# SimpleBench Open Source Release Research

This document covers all requirements and recommendations for a solid open source release on GitHub and crates.io.

## Current State Audit

**Missing (Required)**:
- [ ] README.md
- [ ] LICENSE file(s)
- [ ] Cargo.toml metadata (description, license, repository, etc.)

**Missing (Recommended)**:
- [ ] CHANGELOG.md
- [ ] GitHub Actions workflow for CI/release
- [ ] Proper flake.nix for NixOS import as package

**Exists but needs update**:
- [ ] flake.nix (currently devShell only, not a package)

---

## 1. Cargo.toml Metadata (Required for crates.io)

### Required Fields

Each crate's `Cargo.toml` needs these for crates.io publishing:

```toml
[package]
name = "simplebench-runtime"
version = "0.1.0"
edition = "2021"

# REQUIRED for crates.io:
description = "A short description of the crate"
license = "MIT OR Apache-2.0"

# Strongly recommended:
repository = "https://github.com/YOUR_USERNAME/simplebench"
homepage = "https://github.com/YOUR_USERNAME/simplebench"
documentation = "https://docs.rs/simplebench-runtime"
readme = "README.md"
keywords = ["benchmark", "microbenchmark", "performance", "testing"]
categories = ["development-tools::profiling", "development-tools::testing"]

# Optional but good:
authors = ["Your Name <email@example.com>"]
rust-version = "1.70"  # MSRV - minimum supported Rust version
```

### Workspace Inheritance

Use workspace-level metadata to reduce duplication:

```toml
# Root Cargo.toml
[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/YOUR_USERNAME/simplebench"
authors = ["Your Name <email@example.com>"]
rust-version = "1.70"

# In each member's Cargo.toml
[package]
name = "simplebench-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Core runtime for SimpleBench microbenchmarking framework"
```

### Descriptions for Each Crate

- **simplebench-runtime**: "Core runtime library for SimpleBench, providing timing, statistics, baseline storage, and configuration for Rust microbenchmarks"
- **simplebench-macros**: "Procedural macros for SimpleBench providing the #[bench] attribute for benchmark registration"
- **cargo-simplebench**: "Cargo subcommand for running SimpleBench microbenchmarks across workspace crates"

---

## 2. License Files

Standard Rust dual-licensing (MIT OR Apache-2.0) is recommended:

**Required files in repo root**:
- `LICENSE-MIT`
- `LICENSE-APACHE`

Or alternatively a single `LICENSE` file with dual license text.

**Why dual-license?**: Following Rust ecosystem convention. MIT is permissive and popular; Apache-2.0 provides patent protection.

---

## 3. README.md Structure

Based on criterion.rs and divan patterns:

```markdown
# SimpleBench

Minimalist microbenchmarking framework for Rust with clear regression detection.

[![Crates.io](https://img.shields.io/crates/v/simplebench-runtime.svg)](https://crates.io/crates/simplebench-runtime)
[![Documentation](https://docs.rs/simplebench-runtime/badge.svg)](https://docs.rs/simplebench-runtime)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)]()

## Features

- Simple `#[bench]` attribute for benchmark registration
- Cross-workspace benchmark discovery and execution
- Built-in regression detection with configurable thresholds
- Low-variance measurements with CPU affinity and warmup
- Historical trend analysis
- CI-friendly mode with pass/fail exit codes

## Quick Start

### Installation

```bash
cargo install cargo-simplebench
```

### Add to Your Crate

```toml
[dev-dependencies]
simplebench-runtime = "0.1"
simplebench-macros = "0.1"
```

### Write Benchmarks

```rust
#[cfg(test)]
mod benchmarks {
    use simplebench_macros::bench;

    #[bench]
    fn my_function_benchmark() {
        // Code to benchmark
        expensive_operation();
    }
}
```

### Run

```bash
cargo simplebench
```

## Configuration

[Configuration options...]

## CI Integration

[CI usage...]

## Comparison with Other Tools

| Feature | SimpleBench | Criterion | Divan |
|---------|------------|-----------|-------|
| ...     | ...        | ...       | ...   |

## License

Dual-licensed under MIT or Apache-2.0 at your option.
```

---

## 4. GitHub Actions Workflow

### CI Workflow (`.github/workflows/ci.yml`)

```yaml
name: CI

on:
  push:
    branches: [master, main]
  pull_request:
    branches: [master, main]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests
        run: cargo test --all

  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - name: Check formatting
        run: cargo fmt --all -- --check

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: Clippy
        run: cargo clippy --all -- -D warnings
```

### Semantic Release Workflow (`.github/workflows/release.yml`)

Two main approaches:

#### Option A: semantic-release with npm (recommended by semantic-release-cargo)

```yaml
name: Release

on:
  push:
    branches: [master, main]

concurrency:
  group: release
  cancel-in-progress: false

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install dependencies
        run: npm install -g semantic-release @semantic-release-cargo/semantic-release-cargo @semantic-release/changelog @semantic-release/git

      - name: Release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: npx semantic-release
```

Requires `.releaserc.json`:

```json
{
  "branches": ["master", "main"],
  "plugins": [
    "@semantic-release/commit-analyzer",
    "@semantic-release/release-notes-generator",
    "@semantic-release/changelog",
    "@semantic-release-cargo/semantic-release-cargo",
    "@semantic-release/github",
    [
      "@semantic-release/git",
      {
        "assets": ["CHANGELOG.md", "**/Cargo.toml", "Cargo.lock"],
        "message": "chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}"
      }
    ]
  ]
}
```

#### Option B: cargo-release (simpler, manual trigger)

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      version:
        description: 'Version to release (e.g., 0.2.0)'
        required: true

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Publish to crates.io
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          cargo publish -p simplebench-runtime
          cargo publish -p simplebench-macros
          cargo publish -p cargo-simplebench
```

### Required GitHub Secrets

- `CARGO_REGISTRY_TOKEN`: API token from crates.io (Settings → API Tokens)

---

## 5. flake.nix for NixOS Import

Current flake only provides a devShell. For package consumption, need to add:

```nix
{
  description = "SimpleBench - Minimalist microbenchmarking for Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Common args for all builds
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          buildInputs = [ ];
          nativeBuildInputs = [ ];
        };

        # Build the cargo-simplebench binary
        cargo-simplebench = craneLib.buildPackage (commonArgs // {
          cargoExtraArgs = "-p cargo-simplebench";
        });

      in {
        packages = {
          default = cargo-simplebench;
          cargo-simplebench = cargo-simplebench;
        };

        # Overlay for adding to NixOS
        overlays.default = final: prev: {
          cargo-simplebench = cargo-simplebench;
        };

        devShell = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            rust-analyzer
            # ... existing dev tools
          ];
        };
      }
    );
}
```

Users can then:

```nix
# In their flake.nix
{
  inputs.simplebench.url = "github:YOUR_USERNAME/simplebench";

  # Add to system packages
  environment.systemPackages = [ inputs.simplebench.packages.${system}.default ];
}
```

---

## 6. Additional Recommended Items

### CHANGELOG.md

Use [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] - YYYY-MM-DD

### Added
- Initial release
- `#[bench]` macro for benchmark registration
- Cross-workspace benchmark discovery
- Baseline storage and comparison
- CPU affinity for reduced variance
- Historical trend analysis
- CI mode with regression detection
```

### .gitignore additions

```gitignore
# Already have
target/
.benches/

# Add
*.log
.DS_Store
```

### Documentation (docs.rs)

Ensure public items have doc comments. docs.rs will auto-build from crates.io:

```rust
//! SimpleBench runtime library
//!
//! This crate provides the core functionality for the SimpleBench
//! microbenchmarking framework.
//!
//! # Example
//! ```rust,ignore
//! use simplebench_runtime::SimpleBench;
//! // ...
//! ```
```

### Cargo.toml badges (optional)

```toml
[badges]
maintenance = { status = "actively-developed" }
```

---

## 7. Publishing Order (Important!)

Crates must be published in dependency order:

1. `simplebench-runtime` (no internal deps)
2. `simplebench-macros` (depends on runtime)
3. `cargo-simplebench` (depends on runtime)

**Wait ~1 minute between publishes** for crates.io index to update.

---

## 8. Pre-Release Checklist

### Before First Publish

- [ ] Choose and verify crate names are available on crates.io
- [ ] Add all required metadata to each Cargo.toml
- [ ] Create LICENSE-MIT and LICENSE-APACHE files
- [ ] Write README.md
- [ ] Add doc comments to public API
- [ ] Create CHANGELOG.md
- [ ] Set up GitHub repository (public)
- [ ] Add CARGO_REGISTRY_TOKEN to GitHub secrets
- [ ] Test `cargo publish --dry-run` for each crate
- [ ] Update flake.nix for package output

### Verification Steps

```bash
# Check metadata is complete
cargo package --list -p simplebench-runtime
cargo package --list -p simplebench-macros
cargo package --list -p cargo-simplebench

# Dry-run publish
cargo publish --dry-run -p simplebench-runtime
cargo publish --dry-run -p simplebench-macros
cargo publish --dry-run -p cargo-simplebench
```

---

## 9. Your Original List + Additions

### Your List:
1. ✓ README for GitHub
2. ✓ Cleanup and write documentation on usage
3. ✓ GitHub workflow for semantic release to crates.io
4. ✓ Update flake.nix for NixOS import

### Additional Items Identified:

5. **License files** - Required for crates.io
6. **Cargo.toml metadata** - Required fields for publishing
7. **CHANGELOG.md** - Tracks version history (auto-generated by semantic-release)
8. **CI workflow** - Tests/fmt/clippy on PRs (separate from release)
9. **Doc comments** - For docs.rs generation
10. **Workspace metadata inheritance** - DRY principle for shared fields
11. **MSRV declaration** - Tell users minimum Rust version
12. **Crate name verification** - Ensure names available before publishing
13. **`.releaserc.json`** - semantic-release configuration

---

## Sources

- [Publishing on crates.io - The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [semantic-release-cargo GitHub](https://github.com/semantic-release-cargo/semantic-release-cargo)
- [A Flake for your Crate](https://hoverbear.org/blog/a-flake-for-your-crate/)
- [rust-overlay by oxalica](https://github.com/oxalica/rust-overlay)
- [Criterion.rs](https://github.com/bheisler/criterion.rs) - Reference structure
- [Divan](https://github.com/nvzqz/divan) - Reference structure
