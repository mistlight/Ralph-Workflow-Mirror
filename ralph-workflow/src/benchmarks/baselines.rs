//! Performance baselines for regression detection.
//!
//! This module defines expected performance characteristics based on
//! measurements from the benchmark suite. Tests can compare against
//! these baselines to detect regressions.

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
    pub const ENTRIES_1000: Self = Self {
        entry_count: 1000,
        heap_size_bytes: 500_000,       // 500 KB
        serialized_size_bytes: 400_000, // 400 KB
        tolerance: 1.2,                 // 20% headroom
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
    pub const ENTRIES_1000: Self = Self {
        entry_count: 1000,
        serialize_us: 5_000,   // 5ms
        deserialize_us: 5_000, // 5ms
        tolerance: 2.0,        // 2x headroom (serialization varies)
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
