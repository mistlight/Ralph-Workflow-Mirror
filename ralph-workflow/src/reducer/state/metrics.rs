// Run-level execution metrics for the pipeline.
//
// This is the single source of truth for all iteration/attempt/retry/fallback statistics.
//
// # Where Metrics Are Updated
//
// Metrics are updated **only** in reducer code paths (`state_reduction/*.rs`):
//
// - `development.rs`: dev_iterations_started, dev_iterations_completed,
//                     dev_attempts_total, analysis_attempts_*, xsd_retry_development
// - `review.rs`: review_passes_started, review_passes_completed, review_runs_total,
//                fix_runs_total, fix_continuations_total, xsd_retry_review, xsd_retry_fix
// - `commit.rs`: commits_created_total, xsd_retry_commit
// - `planning.rs`: xsd_retry_planning
// - `agent.rs`: same_agent_retry_attempts_total, agent_fallbacks_total, model_fallbacks_total, retry_cycles_started_total
//
// # How to Add New Metrics
//
// 1. Add field to `RunMetrics` struct with `#[serde(default)]`
// 2. Update `RunMetrics::new()` if config-derived display field
// 3. Update appropriate reducer in `state_reduction/` to increment on event
// 4. Add unit test in `state_reduction/tests/metrics.rs`
// 5. Update `finalize_pipeline()` if displayed in final summary
// 6. Add checkpoint compatibility test
//
// # Checkpoint Compatibility
//
// All fields have `#[serde(default)]` to ensure old checkpoints can be loaded
// with new metrics fields defaulting to 0.

/// Run-level execution metrics tracked by the reducer.
///
/// This struct provides a complete picture of pipeline execution progress,
/// including iteration counts, attempt counts, retry counts, and fallback events.
/// All fields are monotonic counters that only increment during a run.
///
/// # Checkpoint Compatibility
///
/// All fields have `#[serde(default)]` to ensure backward compatibility when
/// loading checkpoints created before metrics were added or when new fields
/// are introduced in future versions.
///
/// # Single Source of Truth
///
/// The reducer is the **only** code that mutates these metrics. They are
/// updated deterministically based on events, ensuring:
/// - Metrics survive checkpoint/resume
/// - No drift between runtime state and actual progress
/// - Final summary is always consistent with reducer state
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunMetrics {
    // Development iteration tracking
    /// Number of development iterations started.
    #[serde(default)]
    pub dev_iterations_started: u32,
    /// Number of development iterations completed (advanced to commit phase).
    #[serde(default)]
    pub dev_iterations_completed: u32,
    /// Total number of developer agent invocations (includes continuations).
    #[serde(default)]
    pub dev_attempts_total: u32,

    // Analysis tracking
    /// Total number of analysis agent invocations across all iterations.
    #[serde(default)]
    pub analysis_attempts_total: u32,
    /// Analysis attempts in the current development iteration (reset per iteration).
    #[serde(default)]
    pub analysis_attempts_in_current_iteration: u32,

    // Review tracking
    /// Number of review passes started.
    #[serde(default)]
    pub review_passes_started: u32,
    /// Number of review passes completed (advanced past without issues or after fixes).
    #[serde(default)]
    pub review_passes_completed: u32,
    /// Total number of reviewer agent invocations.
    #[serde(default)]
    pub review_runs_total: u32,
    /// Total number of fix agent invocations.
    #[serde(default)]
    pub fix_runs_total: u32,
    /// Total number of fix continuation attempts.
    #[serde(default)]
    pub fix_continuations_total: u32,

    // XSD retry tracking
    /// Total XSD retry attempts across all phases.
    #[serde(default)]
    pub xsd_retry_attempts_total: u32,
    /// XSD retry attempts in planning phase.
    #[serde(default)]
    pub xsd_retry_planning: u32,
    /// XSD retry attempts in development/analysis phase.
    #[serde(default)]
    pub xsd_retry_development: u32,
    /// XSD retry attempts in review phase.
    #[serde(default)]
    pub xsd_retry_review: u32,
    /// XSD retry attempts in fix phase.
    #[serde(default)]
    pub xsd_retry_fix: u32,
    /// XSD retry attempts in commit phase.
    #[serde(default)]
    pub xsd_retry_commit: u32,

    // Same-agent retry tracking
    /// Total same-agent retry attempts (for transient failures like timeout).
    #[serde(default)]
    pub same_agent_retry_attempts_total: u32,

    // Agent/model fallback tracking
    /// Total agent fallback events.
    #[serde(default)]
    pub agent_fallbacks_total: u32,
    /// Total model fallback events.
    #[serde(default)]
    pub model_fallbacks_total: u32,
    /// Total retry cycles started (agent chain exhaustion + restart).
    #[serde(default)]
    pub retry_cycles_started_total: u32,

    // Commit tracking
    /// Total commits created during the run.
    #[serde(default)]
    pub commits_created_total: u32,

    // Config-derived display fields (set once at init, not serialized from events)
    /// Maximum development iterations (from config, for X/Y display).
    #[serde(default)]
    pub max_dev_iterations: u32,
    /// Maximum review passes (from config, for X/Y display).
    #[serde(default)]
    pub max_review_passes: u32,
}

impl RunMetrics {
    /// Create metrics with config-derived display fields.
    pub fn new(max_dev_iterations: u32, max_review_passes: u32) -> Self {
        Self {
            max_dev_iterations,
            max_review_passes,
            ..Self::default()
        }
    }
}
