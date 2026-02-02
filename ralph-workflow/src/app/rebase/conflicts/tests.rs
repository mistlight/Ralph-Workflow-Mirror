use super::*;
use crate::executor::MockProcessExecutor;
use crate::logger::{Colors, Logger};
use crate::workspace::{MemoryWorkspace, Workspace};
use std::path::Path;
use std::sync::Arc;

#[test]
fn run_ai_conflict_resolution_uses_unique_logfile_with_attempt_index() {
    let config = crate::config::Config::default();
    let registry = crate::agents::AgentRegistry::new().expect("registry");

    let colors = Colors { enabled: false };
    let logger = Logger::new(colors);

    let executor: Arc<MockProcessExecutor> = Arc::new(MockProcessExecutor::new());

    let workspace = MemoryWorkspace::new_test();
    workspace
        .create_dir_all(Path::new(".agent/logs/rebase_conflict_resolution"))
        .expect("create log dir");
    workspace
        .write(
            Path::new(".agent/logs/rebase_conflict_resolution/conflict_resolution_codex_0_a0.log"),
            "old",
        )
        .expect("write existing log");

    let _ = run_ai_conflict_resolution(
        "resolve conflicts",
        &config,
        &registry,
        &logger,
        colors,
        Arc::clone(&executor) as Arc<dyn crate::executor::ProcessExecutor>,
        &workspace,
    )
    .expect("run");

    let calls = executor.agent_calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0]
        .logfile
        .ends_with("rebase_conflict_resolution/conflict_resolution_codex_0_a1.log"));
}
