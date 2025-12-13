## [1.0.4](https://github.com/CmPons/SimpleBench/compare/v1.0.3...v1.0.4) (2025-12-13)


### Bug Fixes

* add readme field to all crates for crates.io display ([d1c1417](https://github.com/CmPons/SimpleBench/commit/d1c1417799f4b2e814352b80eeda8f80a75226a8))

## [1.0.3](https://github.com/CmPons/SimpleBench/compare/v1.0.2...v1.0.3) (2025-12-13)


### Bug Fixes

* remove accidentally committed target directory and Cargo.lock ([bd27307](https://github.com/CmPons/SimpleBench/commit/bd273070451d2ed278106616e88fe90190dff8ff))

## [1.0.2](https://github.com/CmPons/SimpleBench/compare/v1.0.1...v1.0.2) (2025-12-13)


### Bug Fixes

* Update README.md ([8ffc3be](https://github.com/CmPons/SimpleBench/commit/8ffc3beb932d42e6fcdaaf9007fd5732ef170cbe))

## [1.0.1](https://github.com/CmPons/SimpleBench/compare/v1.0.0...v1.0.1) (2025-12-13)


### Bug Fixes

* Add proper email for authors and crates.io publish ([4a00507](https://github.com/CmPons/SimpleBench/commit/4a005071227a0feebadd136c78ace1458e3aba5a))

# 1.0.0 (2025-12-13)


### Bug Fixes

* add cpu_samples field to test BenchResult instances ([07c6341](https://github.com/CmPons/SimpleBench/commit/07c6341de6c3c8b60c22cc9e6aa0241ecd64d659))
* allow GITHUB_TOKEN to push commits in release workflow ([c810e10](https://github.com/CmPons/SimpleBench/commit/c810e10ab41ecc1786f98da58a8c32821e1419ca))
* Fix tests from not compiling ([a710934](https://github.com/CmPons/SimpleBench/commit/a71093473e8f1a41c3ba7b3bbfa4b3c54bfae600))
* **regression:** use consistent threshold for regression detection ([80efe55](https://github.com/CmPons/SimpleBench/commit/80efe55a70944c1457afe1633a6ce16f732b8a41))
* Remove sccache since it sometimes causes cargo to fail to compile ([c200025](https://github.com/CmPons/SimpleBench/commit/c200025526b96c880d357c564d90a59409dcd31a))
* Set sane defaults for samples and iterations ([f443a21](https://github.com/CmPons/SimpleBench/commit/f443a213bb447575c9a4c5242ec6f15657614192))
* use cargo-release for workspace version bumping ([d6f3cc5](https://github.com/CmPons/SimpleBench/commit/d6f3cc5f951e8ec78bbbfd867f37e729c11f2d2b))
* use runtime measurement functions in generated runner ([5d0c9ef](https://github.com/CmPons/SimpleBench/commit/5d0c9ef2cf9328df67b160acdfbc145830871856))
* use workspace dependencies for inter-crate version sync ([fbe9af5](https://github.com/CmPons/SimpleBench/commit/fbe9af504e6c69e6d8156930d8262840f0db52ac))


### Features

* add 'run' command with benchmark filtering (Stage 5) ([193c7fe](https://github.com/CmPons/SimpleBench/commit/193c7fee91c81f81d28d27c1e66552333cafc58e))
* add colored terminal output and improve formatting ([edbac1f](https://github.com/CmPons/SimpleBench/commit/edbac1f918deb780d2821b18e99c6156764c2d32))
* add CPU context to analyze command (Stage 3 part 2) ([08b72b3](https://github.com/CmPons/SimpleBench/commit/08b72b3c31d0464e4586c9daa28e20b642941c49))
* **analysis:** add historical data tracking and analysis tools ([0b7f45a](https://github.com/CmPons/SimpleBench/commit/0b7f45a6a0c908922a4618b808b418e6dbd6d311))
* **baseline:** migrate from hostname to MAC address for machine identification ([bdfa953](https://github.com/CmPons/SimpleBench/commit/bdfa9537a1b376ff8e34e28f86612c86af288663))
* implement Bayesian Change Point Detection with regression filtering ([6169a69](https://github.com/CmPons/SimpleBench/commit/6169a6905c38166e6680eccc96298e95c77c7358))
* implement cargo-simplebench CLI tool (Phase 4) ([dc11e18](https://github.com/CmPons/SimpleBench/commit/dc11e18a4a8c54ceba346e3de739ac0f409bc4d2))
* implement cfg(test) conditional compilation with isolated target dir ([0529bf7](https://github.com/CmPons/SimpleBench/commit/0529bf7b4eb7f78334ce775466c2bb99f5950afa))
* implement CPU monitoring, analysis, and time-based warmup (Stages 1-4) ([12367ac](https://github.com/CmPons/SimpleBench/commit/12367acf945a8fce91798555c053179964d74174))
* implement dev-dependency support for benchmark crates ([d97d97f](https://github.com/CmPons/SimpleBench/commit/d97d97f792321f9a0c0e3524be5cdfbd0bc7d2f8))
* implement parallel benchmark execution with streaming output ([e934a36](https://github.com/CmPons/SimpleBench/commit/e934a36a7872e06b6e914761b476e30f444fdc4c))
* implement Phase 1 variance reduction (0-5% variance achieved) ([c84971a](https://github.com/CmPons/SimpleBench/commit/c84971a32ed13c00c8ea32ca9187890692db9f85))
* implement simplebench runtime and macro crates (Phase 1 & 2) ([e9053c6](https://github.com/CmPons/SimpleBench/commit/e9053c614b6ae6f9af6872e8d398c88fc01f787b))
* improve CLI output formatting ([4a25314](https://github.com/CmPons/SimpleBench/commit/4a25314642e83fa088116fa2d5735c47a8d1ac3f))
* **phase1:** complete dynamic iteration scaling and validation (Tasks 3-4) ([2a29049](https://github.com/CmPons/SimpleBench/commit/2a2904930bfecefe26131a8dbaa26de049c8d204))
* **phase1:** implement configuration system and enable warmup (Task 0) ([4a0ee1b](https://github.com/CmPons/SimpleBench/commit/4a0ee1b157d71581f8511b743dabf6e40c648313))
* prepare for open source release (crates.io + GitHub) ([0c67905](https://github.com/CmPons/SimpleBench/commit/0c67905579208d2e427efb0f37b308abf297fbf2))
* **privacy:** hash MAC addresses for baseline storage ([2bf5baf](https://github.com/CmPons/SimpleBench/commit/2bf5baffec85e7d73a5be9f89d64ee12c5002a1f))
* Set affinity on startup to core 0 ([a207a22](https://github.com/CmPons/SimpleBench/commit/a207a22fdf0dffb0c7d2a1b58989b7d08aecd93f))
* standardize on bench profile with #[bench] attribute ([771c565](https://github.com/CmPons/SimpleBench/commit/771c565706b7a8837ef7dcbb574c2dea8ebb8db9))


### BREAKING CHANGES

* - Removed global --samples, --iterations, --warmup-iterations flags
- Use 'run' subcommand for these options: cargo simplebench run --samples 500

Tested:
- Filter matching works (substring match on benchmark name)
- Backward compatibility: 'cargo simplebench' still works
- Shows clear filter statistics in output

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
* **analysis:** Baseline storage format updated from single file per
benchmark to timestamped runs in directories. Legacy format still
supported for backward compatibility.

Features added:

- Full sample retention: All raw timing samples now stored in JSON
  (previously only percentiles were saved)
- New Statistics struct with comprehensive metrics (mean, median, p90,
  p99, std_dev, variance, min, max, sample_count)
- Timestamp-based storage: Each run creates a new timestamped file
  instead of overwriting (.benches/<id>/<crate>_<bench>/<timestamp>.json)
- analyze subcommand: Rich analysis tool for benchmark data
  - Basic usage: cargo simplebench analyze <benchmark_name>
  - Specific run: --run <timestamp>
  - Historical comparison: --last N
- Outlier detection: IQR (1.5× threshold) and Z-score (3σ) methods
- Variance reporting: CV% now displayed in benchmark output
- Clean presentation: Simple dividing lines with colored text instead
  of bordered boxes for better readability

Implementation details:

Runtime (simplebench-runtime):
- Updated BaselineData to store full sample arrays as Vec<u128>
- Added calculate_statistics() for comprehensive stat calculations
- Modified BaselineManager for timestamp-based directory structure
- Added list_runs() and load_run() for historical data access
- Maintained backward compatibility with legacy single-file format

CLI (cargo-simplebench):
- Added analyze subcommand with clap Subcommand enum
- Created analyze.rs module with statistical analysis and visualization
- Added simplebench-runtime as dependency for BaselineManager access
- Updated main.rs to route analyze subcommand to analysis module

Output improvements:
- Benchmark results now show CV% (coefficient of variation)
- Analysis output uses clean dividing lines and colored text
- Proper indentation hierarchy for readability
- No alignment issues with ANSI color codes

Storage efficiency:
- ~10 bytes per sample in JSON format
- 100K samples = ~978 KB per run (efficient compression)

All tests passing (25 tests in simplebench-runtime).

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>

# Changelog

All notable changes to this project will be documented in this file.

This file is automatically maintained by [semantic-release](https://github.com/semantic-release/semantic-release).
