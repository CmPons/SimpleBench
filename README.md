# SimpleBench

[![Crates.io](https://img.shields.io/crates/v/cargo-simplebench.svg)](https://crates.io/crates/cargo-simplebench)
[![Documentation](https://docs.rs/simplebench-runtime/badge.svg)](https://docs.rs/simplebench-runtime)
[![License](https://img.shields.io/crates/l/cargo-simplebench.svg)](https://github.com/CmPons/simplebench#license)

A minimalist microbenchmarking framework for Rust with clear regression detection.

SimpleBench provides a simple `#[bench]` attribute and automatic workspace-wide benchmark discovery, without the complexity of larger frameworks.

## Features

- **Simple API** - Just add `#[bench]` to any function
- **Workspace support** - Automatically discovers and runs benchmarks across all crates
- **Regression detection** - Statistical comparison against baselines with configurable thresholds
- **Low variance** - CPU pinning, warmup phases, and high sample counts for reliable measurements
- **CI-ready** - `--ci` flag returns non-zero exit code on regressions
- **Historical tracking** - Stores run history for trend analysis

## Quick Start

### Installation

```bash
cargo install cargo-simplebench
```

### Add Dependencies

In your crate's `Cargo.toml`:

```toml
[dev-dependencies]
simplebench-runtime = "0.1"
simplebench-macros = "0.1"
```

### Write a Benchmark

```rust
#[cfg(test)]
mod benchmarks {
    use simplebench_macros::bench;

    #[bench]
    fn my_function() {
        // Code to benchmark
        let sum: u64 = (0..1000).sum();
        std::hint::black_box(sum);
    }
}
```

### Run Benchmarks

```bash
cargo simplebench
```

## Configuration

### Command Line Options

```bash
cargo simplebench [OPTIONS]

Options:
  --samples <N>           Number of samples per benchmark (default: 1000)
  --iterations <N>        Iterations per sample (default: 1000)
  --warmup-duration <S>   Warmup duration in seconds (default: 5)
  --threshold <P>         Regression threshold percentage (default: 5.0)
  --ci                    CI mode - exit with error on regression
  --filter <PATTERN>      Run only benchmarks matching pattern
```

### Environment Variables

All options can also be set via environment variables:

- `SIMPLEBENCH_SAMPLES`
- `SIMPLEBENCH_ITERATIONS`
- `SIMPLEBENCH_WARMUP_DURATION`
- `SIMPLEBENCH_THRESHOLD`
- `SIMPLEBENCH_BENCH_FILTER`

### Configuration File

Create `.simplebench.toml` in your project root:

```toml
[measurement]
samples = 1000
iterations = 1000
warmup_duration_secs = 5

[comparison]
threshold = 5.0
```

## CI Integration

```yaml
# .github/workflows/bench.yml
name: Benchmarks
on: [push, pull_request]

jobs:
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-simplebench
      - run: cargo simplebench --ci
```

## Subcommands

```bash
# Run benchmarks
cargo simplebench

# Clean baseline data
cargo simplebench clean

# Analyze historical trends
cargo simplebench analyze <benchmark_name> --last 10
```

## How It Works

SimpleBench uses the `inventory` crate for compile-time benchmark registration. The `#[bench]` macro expands to register each benchmark function, and `cargo simplebench` builds a unified runner that links all workspace crates and executes discovered benchmarks.

Benchmarks are compiled with `#[cfg(test)]`, so they're excluded from production builds.

## Crates

- [`cargo-simplebench`](https://crates.io/crates/cargo-simplebench) - CLI tool
- [`simplebench-runtime`](https://crates.io/crates/simplebench-runtime) - Core runtime library
- [`simplebench-macros`](https://crates.io/crates/simplebench-macros) - `#[bench]` proc-macro

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
