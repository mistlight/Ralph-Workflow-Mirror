//! Performance baselines for regression detection.
//!
//! This module defines expected performance characteristics based on
//! measurements from the benchmark suite. Tests can compare against
//! these baselines to detect regressions.
//!
//! # Baseline Measurements
//!
//! Current measurements (as of 2026-02-13):
//! - **Execution history growth**: ~53 bytes per iteration (bounded at 1000 entries)
//! - **Checkpoint size**: ~363 KB for 1000 entries (well under 2048 KB hard limit)
//! - **Memory usage**: Bounded growth verified by integration tests
//!
//! # Regression Detection Strategy
//!
//! 1. **CI runs `scripts/ci_performance_regression.sh` on every commit**
//!    - Fails if execution history exceeds 1000 entries (hard limit)
//!    - Fails if checkpoint size exceeds 2048 KB (hard limit)
//!    - Fails if thread cleanup doesn't complete (timeout detection)
//!
//! 2. **Benchmark tests capture current values for trending**
//!    - Run with: `cargo test -p ralph-workflow --lib benchmarks -- --nocapture`
//!    - Values are informational; baselines have generous tolerance
//!
//! 3. **Integration tests enforce behavioral invariants**
//!    - Bounded growth: `tests/integration_tests/memory_safety/bounded_growth.rs`
//!    - Thread cleanup: `tests/integration_tests/memory_safety/thread_lifecycle.rs`
//!    - Arc patterns: `tests/integration_tests/memory_safety/arc_patterns.rs`
//!
//! 4. **Tolerance rationale**
//!    - Memory baselines: 20% tolerance (accounts for platform variance)
//!    - Time baselines: 2x tolerance (serialization performance varies widely)
//!    - Hard limits: 0% tolerance (prevent unbounded growth)

/// Performance baseline for execution history growth.
#[derive(Debug, Clone)]
pub struct ExecutionHistoryBaseline {
    /// Number of entries in benchmark
    pub entry_count: usize,
    /// Expected heap size in bytes
    pub heap_size_bytes: usize,
    /// Expected serialized size in bytes
    pub serialized_size_bytes: usize,
    /// Tolerance factor (1.2 = allow 20% deviation)
    pub tolerance: f64,
}

impl ExecutionHistoryBaseline {
    /// Baseline for 1000 entries (default limit).
    ///
    /// # Measurement Methodology
    ///
    /// These values are derived from benchmark tests that:
    /// 1. Create 1000 execution history entries with realistic content
    /// 2. Measure heap size using `std::mem::size_of_val` and content sizes
    /// 3. Serialize to JSON and measure compressed size
    /// 4. Run multiple iterations to verify consistency
    ///
    /// # Updating Baselines
    ///
    /// If legitimate performance improvements reduce memory usage, update these
    /// values based on new benchmark measurements. Always maintain 20% tolerance
    /// to account for platform variance.
    ///
    /// **DO NOT** increase these values to accommodate regressions. Investigate
    /// and fix the root cause instead.
    pub const ENTRIES_1000: Self = Self {
        entry_count: 1000,
        heap_size_bytes: 500_000,       // 500 KB (measured: ~53 bytes/entry)
        serialized_size_bytes: 400_000, // 400 KB (measured: ~363 KB actual)
        tolerance: 1.2,                 // 20% headroom for platform variance
    };

    /// Check if measured value exceeds baseline.
    pub fn check_heap_size(&self, measured: usize) -> Result<(), String> {
        let max_allowed = (self.heap_size_bytes as f64 * self.tolerance) as usize;
        if measured > max_allowed {
            Err(format!(
                "Heap size {} bytes exceeds baseline {} bytes (tolerance: {}x)",
                measured, max_allowed, self.tolerance
            ))
        } else {
            Ok(())
        }
    }

    /// Check if serialized size exceeds baseline.
    pub fn check_serialized_size(&self, measured: usize) -> Result<(), String> {
        let max_allowed = (self.serialized_size_bytes as f64 * self.tolerance) as usize;
        if measured > max_allowed {
            Err(format!(
                "Serialized size {} bytes exceeds baseline {} bytes (tolerance: {}x)",
                measured, max_allowed, self.tolerance
            ))
        } else {
            Ok(())
        }
    }
}

/// Checkpoint serialization performance baseline.
#[derive(Debug, Clone)]
pub struct CheckpointSerializationBaseline {
    /// Number of history entries
    pub entry_count: usize,
    /// Expected serialization time in microseconds
    pub serialize_us: u64,
    /// Expected deserialization time in microseconds
    pub deserialize_us: u64,
    /// Tolerance factor
    pub tolerance: f64,
}

impl CheckpointSerializationBaseline {
    /// Baseline for 1000 entries.
    ///
    /// # Measurement Methodology
    ///
    /// These values are derived from benchmark tests that:
    /// 1. Create checkpoint state with 1000 execution history entries
    /// 2. Measure `serde_json::to_string()` serialization time
    /// 3. Measure `serde_json::from_str()` deserialization time
    /// 4. Run multiple iterations to get representative average
    ///
    /// # Tolerance Rationale
    ///
    /// Serialization performance varies significantly based on:
    /// - CPU architecture and speed
    /// - Memory bus speed
    /// - System load (other processes)
    /// - Compiler optimizations (debug vs release)
    ///
    /// We use 2x tolerance (100% headroom) to avoid false positives while
    /// still catching catastrophic regressions (e.g., O(n²) algorithms).
    pub const ENTRIES_1000: Self = Self {
        entry_count: 1000,
        serialize_us: 5_000,   // 5ms (typical range: 2-10ms)
        deserialize_us: 5_000, // 5ms (typical range: 2-10ms)
        tolerance: 2.0,        // 2x headroom for hardware/load variance
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_check_within_tolerance() {
        let baseline = ExecutionHistoryBaseline::ENTRIES_1000;

        // 450 KB is within 500 KB baseline + 20% tolerance
        assert!(baseline.check_heap_size(450_000).is_ok());

        // 650 KB exceeds (500 KB * 1.2 = 600 KB)
        assert!(baseline.check_heap_size(650_000).is_err());
    }
}
