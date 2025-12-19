//! Progress reporting for benchmark execution.
//!
//! Emits JSON progress messages to stderr during warmup and sample collection.
//! The CLI tool parses these to display progress bars.

use serde::Serialize;

/// Progress phase during benchmark execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase")]
pub enum ProgressPhase {
    /// Warmup phase - running iterations to stabilize CPU/cache state.
    #[serde(rename = "warmup")]
    Warmup {
        /// Milliseconds elapsed so far.
        elapsed_ms: u64,
        /// Target warmup duration in milliseconds.
        target_ms: u64,
    },
    /// Sample collection phase.
    #[serde(rename = "samples")]
    Samples {
        /// Current sample index (0-based).
        current: u32,
        /// Total number of samples to collect.
        total: u32,
    },
    /// Benchmark complete.
    #[serde(rename = "complete")]
    Complete,
}

/// Progress message emitted to stderr during benchmark execution.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressMessage<'a> {
    /// Benchmark name.
    pub bench: &'a str,
    /// Current phase and its data.
    #[serde(flatten)]
    pub phase: ProgressPhase,
}

/// Check if progress output is enabled (not suppressed via env var).
fn progress_enabled() -> bool {
    std::env::var("SIMPLEBENCH_QUIET").is_err()
}

/// Emit progress JSON to stderr (non-blocking, fire-and-forget).
///
/// Output is wrapped in `{"progress": ...}` envelope to distinguish
/// from other stderr output (warnings, errors).
pub fn emit_progress(msg: &ProgressMessage) {
    if !progress_enabled() {
        return;
    }

    // Wrap in {"progress": ...} envelope
    if let Ok(json) = serde_json::to_string(&serde_json::json!({"progress": msg})) {
        eprintln!("{}", json);
    }
}
