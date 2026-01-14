// This file now re-exports the split test modules for backward compatibility
// The original 2471-line file has been split into 5 focused test modules:
// - workflow_plan_tests.rs: Plan-related tests
// - workflow_review_tests.rs: Review cycle tests
// - workflow_commit_tests.rs: Commit behavior tests
// - workflow_config_tests.rs: Config and initialization tests
// - workflow_cleanup_tests.rs: Cleanup and error recovery tests

mod workflow_plan_tests;
mod workflow_review_tests;
mod workflow_commit_tests;
mod workflow_config_tests;
mod workflow_cleanup_tests;
