// Hand-written unified test runner for Phase 3 investigation
// This file demonstrates the unified runner approach

// Import all workspace crates to trigger inventory collection
// Using `use` statements to force the linker to include inventory submissions
use game_math;
use game_entities;
use game_physics;

// Import simplebench runtime
use simplebench_runtime;

fn main() {
    println!("=== SimpleBench Phase 3 Manual Test Runner ===\n");

    // Run all benchmarks collected via inventory
    // Using fixed parameters: 100 iterations per sample, 100 samples
    let results = simplebench_runtime::run_all_benchmarks(100, 100);

    println!("Found {} benchmarks across all crates:\n", results.len());

    for result in &results {
        println!("Benchmark: {}::{}", result.module, result.name);
        println!("  Iterations: {} × {} samples", result.iterations, result.samples);
        println!("  p50: {:?}", result.percentiles.p50);
        println!("  p90: {:?}", result.percentiles.p90);
        println!("  p99: {:?}", result.percentiles.p99);
        println!();
    }

    // Output as JSON for verification
    match serde_json::to_string_pretty(&results) {
        Ok(json) => {
            println!("\n=== JSON Output ===");
            println!("{}", json);
        }
        Err(e) => {
            eprintln!("Error serializing results: {}", e);
        }
    }
}
