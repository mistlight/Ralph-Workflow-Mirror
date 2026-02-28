use super::super::common::TestFixture;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::workspace::Workspace;
use std::path::PathBuf;

#[test]
fn test_check_commit_diff_emits_prepared_event() {
    use crate::reducer::prompt_inputs::sha256_hex_str;

    let mut fixture = TestFixture::new();
    let ctx = fixture.ctx();

    let result = MainEffectHandler::check_commit_diff_with_content(&ctx, "")
        .expect("check_commit_diff_with_content should succeed");

    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffPrepared {
            empty: true,
            content_id_sha256,
        }) if content_id_sha256 == sha256_hex_str("")
    ));
}

#[test]
fn test_check_commit_diff_emits_failed_event_on_error() {
    let mut fixture = TestFixture::new();
    let ctx = fixture.ctx();

    let result =
        MainEffectHandler::check_commit_diff_with_result(&ctx, Err(anyhow::anyhow!("diff failed")))
            .expect("check_commit_diff_with_result should succeed");

    // New behavior: diff failure uses fallback instructions instead of DiffFailed event
    // The event should be DiffPrepared with fallback content
    assert!(matches!(
        result.event,
        PipelineEvent::Commit(crate::reducer::event::CommitEvent::DiffPrepared { .. })
    ));
}

#[test]
fn test_check_commit_diff_discovers_repo_from_ctx_repo_root_not_process_cwd() {
    use std::path::Path;

    struct RestoreCwd {
        original: PathBuf,
    }
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    let mut fixture = TestFixture::new();
    fixture.repo_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let _restore = RestoreCwd {
        original: std::env::current_dir().unwrap(),
    };
    std::env::set_current_dir(std::env::temp_dir()).unwrap();

    let ctx = fixture.ctx();

    let _result = MainEffectHandler::check_commit_diff(&ctx)
        .expect("check_commit_diff should succeed when repo_root is set");

    let diff = fixture
        .workspace
        .read(Path::new(".agent/tmp/commit_diff.txt"))
        .expect("expected commit diff file to be written");
    assert!(
        !diff.starts_with("## DIFF UNAVAILABLE - INVESTIGATION REQUIRED"),
        "Diff should be computed from ctx.repo_root even when process CWD is elsewhere"
    );
}
