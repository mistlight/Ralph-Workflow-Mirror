use super::super::common::TestFixture;
use crate::reducer::event::ErrorEvent;
use crate::reducer::handler::MainEffectHandler;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_create_commit_returns_typed_error_event_when_git_add_all_fails() {
    let mut fixture = TestFixture::new();
    // Use a unique, non-existent repo root so git discovery fails deterministically.
    // This avoids mutating process-wide CWD (which would be flaky under parallel test execution).
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    fixture.repo_root = std::env::temp_dir().join(format!("ralph-nonexistent-repo-{unique}"));

    let ctx = fixture.ctx();

    let err = MainEffectHandler::create_commit(&ctx, "test message".to_string())
        .expect_err("create_commit should fail when repo discovery fails");

    assert!(
        err.downcast_ref::<ErrorEvent>().is_some(),
        "expected Err() to carry an ErrorEvent, got: {err:?}"
    );
}
