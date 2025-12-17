# SimpleBench Open Source Release - Implementation Plan

## Phase 1: Cargo.toml Metadata

Add required crates.io metadata to all workspace crates.

### 1.1 Root Cargo.toml - Add workspace metadata

```toml
[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/USERNAME/simplebench"
authors = ["Your Name <email>"]
rust-version = "1.70"
```

### 1.2 Update each crate's Cargo.toml

- `simplebench-runtime/Cargo.toml` - inherit workspace fields, add description + keywords
- `simplebench-macros/Cargo.toml` - inherit workspace fields, add description + keywords
- `cargo-simplebench/Cargo.toml` - inherit workspace fields, add description + keywords

---

## Phase 2: README.md

### 2.1 Create README.md in repo root

Sections:
- Title + badges (crates.io, docs.rs, license)
- Brief description
- Features list
- Quick start (install, add deps, write benchmark, run)
- Configuration options
- CI integration example
- License footer

---

## Phase 3: GitHub Workflows

### 3.1 Create `.github/workflows/ci.yml`

Runs on push/PR to master:
- `cargo test --all`
- `cargo fmt --check`
- `cargo clippy`

### 3.2 Create `.releaserc.json`

Configure semantic-release plugins:
- commit-analyzer
- release-notes-generator
- changelog
- semantic-release-cargo
- github
- git (commit updated Cargo.toml + CHANGELOG)

### 3.3 Create `.github/workflows/release.yml`

Manual trigger (`workflow_dispatch`) that:
- Runs semantic-release with cargo plugin
- Bumps versions in Cargo.toml files
- Publishes to crates.io (when secrets configured)
- Creates GitHub release + tag

---

## Phase 4: Nix Flake Update

### 4.1 Update flake.nix

Add package outputs using crane so users can:
```nix
inputs.simplebench.url = "github:USERNAME/simplebench";
environment.systemPackages = [ inputs.simplebench.packages.${system}.default ];
```

---

## Phase 5: Documentation

### 5.1 Add doc comments to public API

Ensure `simplebench-runtime/src/lib.rs` public items have `///` docs for docs.rs.

### 5.2 Create CHANGELOG.md

Initial skeleton - semantic-release will manage this going forward.

---

## Phase 6: Verification

### 6.1 Local checks

```bash
# Verify metadata complete
cargo publish --dry-run -p simplebench-runtime
cargo publish --dry-run -p simplebench-macros
cargo publish --dry-run -p cargo-simplebench

# Verify tests pass
cargo test --all

# Verify formatting
cargo fmt --check
cargo clippy
```

### 6.2 Push and test CI workflow

- Push to master
- Verify CI workflow runs successfully

### 6.3 Test release workflow (dry-run)

- Manually trigger release workflow
- Verify it runs through version detection
- Will fail at publish step (no secrets) - expected

---

## Phase 7: Go Live

### 7.1 Configure GitHub secrets

- Add `CARGO_REGISTRY_TOKEN` from crates.io

### 7.2 Make repo public

### 7.3 Trigger release workflow

- Creates first release (0.1.0 or as determined by commits)
- Publishes all three crates to crates.io

---

## Task Order

| # | Task | Depends On |
|---|------|------------|
| 1 | Workspace metadata in root Cargo.toml | - |
| 2 | Update simplebench-runtime Cargo.toml | 1 |
| 3 | Update simplebench-macros Cargo.toml | 1 |
| 4 | Update cargo-simplebench Cargo.toml | 1 |
| 5 | Create README.md | - |
| 6 | Create .github/workflows/ci.yml | - |
| 7 | Create .releaserc.json | - |
| 8 | Create .github/workflows/release.yml | 7 |
| 9 | Update flake.nix with package output | - |
| 10 | Add doc comments to public API | - |
| 11 | Create CHANGELOG.md | - |
| 12 | Run local verification checks | 1-11 |
| 13 | Push and verify CI | 12 |
| 14 | Test release workflow (will fail at publish) | 13 |
| 15 | Add CARGO_REGISTRY_TOKEN secret | 14 |
| 16 | Make repo public | 15 |
| 17 | Trigger release - publish to crates.io | 16 |

---

## Notes

- Crate names to verify available: `simplebench-runtime`, `simplebench-macros`, `cargo-simplebench`
- Publishing order enforced by semantic-release-cargo: runtime → macros → cargo-simplebench
- Manual workflow trigger means you control exactly when releases happen
- Licenses will be added via GitHub UI
