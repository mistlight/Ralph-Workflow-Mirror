// Tests for deduplication module.

#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/rolling_hash_window.rs");
    include!("tests/kmp_matcher.rs");
    include!("tests/delta_deduplicator.rs");
    include!("tests/overlap_thresholds.rs");
}
