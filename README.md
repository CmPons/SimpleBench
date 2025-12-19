# SimpleBench

[![Crates.io](https://img.shields.io/crates/v/cargo-simplebench.svg)](https://crates.io/crates/cargo-simplebench)
[![Documentation](https://docs.rs/simplebench-runtime/badge.svg)](https://docs.rs/simplebench-runtime)

A minimalist microbenchmarking framework for Rust with clear regression detection.

SimpleBench provides a simple `#[bench]` attribute and automatic workspace-wide benchmark discovery, without the complexity of larger frameworks.

## Features

- **Simple API** - Just add `#[bench]` to any function
- **Workspace support** - Automatically discovers and runs benchmarks across all crates
- **Regression detection** - Statistical comparison against baselines with configurable thresholds
- **Low variance** - CPU pinning, warmup phases, and high sample counts for reliable measurements
- **Progress feedback** - Real-time progress bars showing warmup and sampling phases
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
simplebench-runtime = "2.1"
simplebench-macros = "2.1"
```

### Write a Benchmark

```rust
#[cfg(test)]
mod benchmarks {
    use simplebench_macros::bench;

    // Simple benchmark - entire function body is measured
    #[bench]
    fn my_function() {
        let sum: u64 = (0..1000).sum();
        std::hint::black_box(sum);
    }
}
```

### Benchmarks with Setup

For benchmarks where setup is expensive, use the `setup` attribute to run setup code **once** before measurement begins:

```rust
#[cfg(test)]
mod benchmarks {
    use simplebench_macros::bench;

    // Setup runs once, benchmark receives reference to data
    #[bench(setup = || generate_test_data(1000))]
    fn benchmark_with_setup(data: &Vec<u64>) {
        let sum: u64 = data.iter().sum();
        std::hint::black_box(sum);
    }

    // Named setup function also works
    fn create_large_dataset() -> Vec<String> {
        (0..10000).map(|i| format!("item_{}", i)).collect()
    }

    #[bench(setup = create_large_dataset)]
    fn benchmark_filtering(data: &Vec<String>) {
        let filtered: Vec<_> = data.iter()
            .filter(|s| s.contains("5"))
            .collect();
        std::hint::black_box(filtered);
    }
}
```

### Per-Sample Setup (`setup_each`)

For benchmarks that need fresh data for each sample (e.g., sorting, consuming iterators), use `setup_each`. The setup runs before **every** sample:

```rust
#[cfg(test)]
mod benchmarks {
    use simplebench_macros::bench;

    // Owning: benchmark takes ownership (for mutations/consumption)
    #[bench(setup_each = || vec![5, 3, 8, 1, 9, 4, 7, 2, 6])]
    fn bench_sort(mut data: Vec<i32>) {
        data.sort();  // Mutates the data
    }

    // Owning: consuming an iterator
    #[bench(setup_each = || (0..1000).collect::<Vec<_>>())]
    fn bench_consume_iter(data: Vec<i32>) {
        let _sum: i32 = data.into_iter().sum();  // Consumes data
    }

    // Borrowing: fresh data each sample, but only reading
    #[bench(setup_each = || random_vectors(1000))]
    fn bench_normalize(vectors: &Vec<Vec3>) {
        for v in vectors {
            let _ = v.normalize();
        }
    }
}
```

**When to use which:**
- `setup` - Setup is expensive and data is read-only (setup runs once)
- `setup_each` with `T` - Benchmark mutates or consumes the data
- `setup_each` with `&T` - Need fresh random/unique data each sample

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
  --warmup-duration <S>   Warmup duration in seconds (default: 3)
  --threshold <P>         Regression threshold percentage (default: 5.0)
  --ci                    CI mode - exit with error on regression
  --bench <PATTERN>       Run only benchmarks matching pattern
  --parallel              Run benchmarks in parallel (faster, may increase variance)
  -j, --jobs <N>          Number of parallel jobs (implies --parallel)
  -q, --quiet             Suppress progress bars
```

### Environment Variables

All options can also be set via environment variables:

- `SIMPLEBENCH_SAMPLES`
- `SIMPLEBENCH_WARMUP_DURATION`
- `SIMPLEBENCH_THRESHOLD`
- `SIMPLEBENCH_BENCH_FILTER`
- `SIMPLEBENCH_QUIET`

### Configuration File

Create `simplebench.toml` in your project root:

```toml
[measurement]
samples = 1000
warmup_duration_secs = 3

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

Each sample measures exactly one function call, giving you per-call timing data with full variance information.

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

---

*This project was co-authored with [Claude](https://claude.ai), an AI assistant by Anthropic.*
