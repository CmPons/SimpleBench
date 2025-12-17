// Test runner for manual verification of benchmark execution
// Note: The main cargo-simplebench tool generates its own runner

use game_entities;
use game_math;
use game_physics;
use simplebench_runtime::{config::BenchmarkConfig, run_and_stream_benchmarks};

fn main() {
    println!("=== SimpleBench Manual Test Runner ===\n");

    // Create a config with reasonable test defaults
    let mut config = BenchmarkConfig::default();
    config.measurement.samples = 10;
    config.measurement.iterations = 100;
    config.measurement.warmup_duration_secs = 1;

    // Suppress unused warnings - these imports are needed to link the benchmarks
    let _ = (game_math::Vec3::new(0.0, 0.0, 0.0), game_entities::Entity::new(0));
    let _ = game_physics::AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

    // Run all benchmarks collected via inventory
    let results = run_and_stream_benchmarks(&config);

    println!("\n=== Summary ===");
    println!("Completed {} benchmarks", results.len());
}
