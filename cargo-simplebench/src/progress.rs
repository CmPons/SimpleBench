//! Progress bar display for benchmark execution.
//!
//! Parses JSON progress messages from runner stderr and displays
//! indicatif progress bars in the terminal.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::IsTerminal;

/// Wrapper for progress messages from the runtime.
#[derive(Debug, Deserialize)]
pub struct ProgressWrapper {
    pub progress: ProgressMessage,
}

/// Progress message from the benchmark runner.
#[derive(Debug, Deserialize)]
pub struct ProgressMessage {
    pub bench: String,
    #[serde(flatten)]
    pub phase: ProgressPhase,
}

/// Progress phase during benchmark execution.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "phase")]
pub enum ProgressPhase {
    #[serde(rename = "warmup")]
    Warmup { elapsed_ms: u64, target_ms: u64 },
    #[serde(rename = "samples")]
    Samples { current: u32, total: u32 },
    #[serde(rename = "complete")]
    Complete,
}

/// Which phase a benchmark is currently in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayPhase {
    Warmup,
    Samples,
}

/// State for a single benchmark's progress bar.
struct BenchState {
    bar: ProgressBar,
    phase: DisplayPhase,
}

/// Manages progress bar display for multiple concurrent benchmarks.
pub struct BenchmarkProgress {
    multi: MultiProgress,
    benches: HashMap<String, BenchState>,
    is_tty: bool,
    quiet: bool,
}

impl BenchmarkProgress {
    /// Create a new progress display manager.
    pub fn new(quiet: bool) -> Self {
        let is_tty = std::io::stderr().is_terminal();
        Self {
            multi: MultiProgress::new(),
            benches: HashMap::new(),
            is_tty,
            quiet,
        }
    }

    /// Check if progress display is enabled.
    pub fn is_enabled(&self) -> bool {
        self.is_tty && !self.quiet
    }

    /// Update progress based on a parsed message.
    pub fn update(&mut self, msg: &ProgressMessage) {
        if !self.is_enabled() {
            return;
        }

        match &msg.phase {
            ProgressPhase::Warmup {
                elapsed_ms,
                target_ms,
            } => {
                self.update_warmup(&msg.bench, *elapsed_ms, *target_ms);
            }
            ProgressPhase::Samples { current, total } => {
                self.update_samples(&msg.bench, *current, *total);
            }
            ProgressPhase::Complete => {
                self.finish_bench(&msg.bench);
            }
        }
    }

    fn update_warmup(&mut self, bench: &str, elapsed_ms: u64, target_ms: u64) {
        let state = self.benches.entry(bench.to_string()).or_insert_with(|| {
            let pb = self.multi.add(ProgressBar::new(target_ms));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{prefix:>20.cyan.bold} [{bar:30.yellow/dim}] {msg}")
                    .unwrap()
                    .progress_chars("━━╺"),
            );
            pb.set_prefix(bench.to_string());
            BenchState {
                bar: pb,
                phase: DisplayPhase::Warmup,
            }
        });

        // If switching from samples back to warmup (shouldn't happen), recreate bar
        if state.phase != DisplayPhase::Warmup {
            state.bar.finish_and_clear();
            let pb = self.multi.add(ProgressBar::new(target_ms));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{prefix:>20.cyan.bold} [{bar:30.yellow/dim}] {msg}")
                    .unwrap()
                    .progress_chars("━━╺"),
            );
            pb.set_prefix(bench.to_string());
            state.bar = pb;
            state.phase = DisplayPhase::Warmup;
        }

        state.bar.set_length(target_ms);
        state.bar.set_position(elapsed_ms);
        state.bar.set_message(format!(
            "warming up {:.1}s / {:.1}s",
            elapsed_ms as f64 / 1000.0,
            target_ms as f64 / 1000.0
        ));
    }

    fn update_samples(&mut self, bench: &str, current: u32, total: u32) {
        let needs_new_bar = self
            .benches
            .get(bench)
            .map(|s| s.phase != DisplayPhase::Samples)
            .unwrap_or(true);

        if needs_new_bar {
            // Remove old bar if exists
            if let Some(old_state) = self.benches.remove(bench) {
                old_state.bar.finish_and_clear();
            }

            // Create new samples bar
            let pb = self.multi.add(ProgressBar::new(total as u64));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{prefix:>20.cyan.bold} [{bar:30.green/dim}] {pos:>5}/{len:5} {msg}")
                    .unwrap()
                    .progress_chars("━━╺"),
            );
            pb.set_prefix(bench.to_string());
            pb.set_message("sampling");
            pb.tick();

            self.benches.insert(
                bench.to_string(),
                BenchState {
                    bar: pb,
                    phase: DisplayPhase::Samples,
                },
            );
        }

        if let Some(state) = self.benches.get(bench) {
            state.bar.set_position(current as u64);
        }
    }

    fn finish_bench(&mut self, bench: &str) {
        if let Some(state) = self.benches.remove(bench) {
            state.bar.finish_and_clear();
        }
    }

    /// Finish and clear all active progress bars.
    pub fn finish(&mut self) {
        for (_, state) in self.benches.drain() {
            state.bar.finish_and_clear();
        }
    }
}

impl Drop for BenchmarkProgress {
    fn drop(&mut self) {
        self.finish();
    }
}
