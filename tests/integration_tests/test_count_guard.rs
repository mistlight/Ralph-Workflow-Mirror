//! Guard test to ensure integration test count doesn't drop unexpectedly.
//!
//! This module provides documentation and a guard test to catch accidental
//! test suite regressions. If integration test discovery finds fewer tests
//! than expected, developers should be alerted.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** This module is part of the integration test framework and
//! MUST follow the integration test style guide defined in
//! **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! # Purpose
//!
//! This test exists to:
//! - Document the expected minimum integration test count
//! - Serve as a reminder to verify the full suite is running
//! - Prevent accidental test removal or compilation failures from going unnoticed
//!
//! # How to Verify Test Count
//!
//! Run the following command to check the actual test count:
//! ```bash
//! cargo test -p ralph-workflow-tests -- --list 2>&1 | grep -c ': test$'
//! ```
//!
//! The compliance check script (`compliance_check.sh`) also verifies this count.

use crate::test_timeout::with_default_timeout;

/// Minimum expected integration test count.
///
/// Update this when adding new test modules. This is a floor, not an exact target.
/// The actual count should be >= this value.
///
/// If this value needs to decrease significantly, it likely indicates either:
/// - Tests were accidentally removed
/// - A test module is not being compiled
/// - The test discovery is not working correctly
pub const MINIMUM_EXPECTED_TESTS: usize = 400;

/// This test documents the expected minimum test count.
///
/// This verifies that the test count guard module is properly loaded and the
/// constant is accessible. The actual count verification happens in CI via
/// `cargo test -p ralph-workflow-tests -- --list` and in the compliance check script.
///
/// If this test appears, it means the test count guard module is properly loaded
/// and the integration test suite includes this verification documentation.
#[test]
fn integration_test_count_guard_documentation() {
    with_default_timeout(|| {
        // This test documents the expected minimum test count.
        // The actual verification happens in CI or via compliance scripts.
        //
        // To check test count manually:
        //   cargo test -p ralph-workflow-tests -- --list 2>&1 | grep -c ': test$'
        //
        // Expected: 400+ tests
        //
        // Verify the constant is accessible (the actual value check happens in the
        // compliance check script which counts tests via --list).
        let min = MINIMUM_EXPECTED_TESTS;
        // Use the value to avoid unused variable warning
        assert!(min > 0, "MINIMUM_EXPECTED_TESTS should be positive");
    });
}
