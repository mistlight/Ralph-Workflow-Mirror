use super::super::common::TestFixture;
use crate::reducer::event::PipelineEvent;
use crate::reducer::handler::MainEffectHandler;
use crate::workspace::{MemoryWorkspace, Workspace};
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

#[test]
fn test_check_commit_diff_uses_head_baseline_not_start_commit() {
    // TDD regression: check_commit_diff must generate its diff from HEAD (working-tree
    // vs last commit), NOT from .agent/start_commit (pipeline-start baseline).
    //
    // Proof strategy:
    //   Commit A: initial commit (baseline)
    //   Commit B: committed change (already in history — must NOT appear in diff)
    //   Change C: uncommitted modification with a unique marker (MUST appear in diff)
    //
    // If HEAD baseline is used: diff shows only C.
    // If start_commit baseline is used: diff shows both B and C.
    //
    // IMPORTANT: Use an isolated tempdir repo; never mutate process CWD (test parallelism).
    use std::path::Path;

    let repo_dir = tempfile::TempDir::new().expect("create temp git repo");
    let repo = git2::Repository::init(repo_dir.path()).expect("init git repo");
    let sig = git2::Signature::now("test", "test@test.com").expect("signature");

    // Commit A: initial state — create two separate tracked files.
    // file_committed will hold the "already committed" change (commit B).
    // file_working will hold the uncommitted working-tree change (C).
    let file_committed = "committed_change_file.txt";
    let file_working = "working_change_file.txt";
    let abs_committed = repo_dir.path().join(file_committed);
    let abs_working = repo_dir.path().join(file_working);
    std::fs::write(&abs_committed, "base content\n").expect("write committed file A");
    std::fs::write(&abs_working, "base content\n").expect("write working file A");
    let mut index = repo.index().expect("open index");
    index
        .add_path(Path::new(file_committed))
        .expect("stage committed file A");
    index
        .add_path(Path::new(file_working))
        .expect("stage working file A");
    index.write().expect("write index A");
    let tree_a = repo
        .find_tree(index.write_tree().expect("write tree A"))
        .expect("find tree A");
    repo.commit(Some("HEAD"), &sig, &sig, "commit A: initial", &tree_a, &[])
        .expect("create commit A");

    // Commit B: modify file_committed only — this becomes part of history.
    // With HEAD baseline, file_committed has NO working-tree changes (HEAD == workdir).
    // With start_commit baseline, file_committed would show committed_marker as added.
    let committed_marker = "COMMITTED_CHANGE_MUST_NOT_APPEAR_IN_DIFF";
    std::fs::write(
        &abs_committed,
        format!("base content\n{committed_marker}\n"),
    )
    .expect("write committed file for commit B");
    let mut index = repo.index().expect("open index for B");
    index
        .add_path(Path::new(file_committed))
        .expect("stage committed file B");
    index.write().expect("write index B");
    let tree_b = repo
        .find_tree(index.write_tree().expect("write tree B"))
        .expect("find tree B");
    let parent_a = repo
        .head()
        .expect("head after A")
        .peel_to_commit()
        .expect("commit A");
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "commit B: committed change",
        &tree_b,
        &[&parent_a],
    )
    .expect("create commit B");

    // Change C: modify file_working without staging (MUST appear in HEAD diff).
    // file_working is tracked (committed in A) but not changed in B, so HEAD still has base content.
    let uncommitted_marker = "UNCOMMITTED_CHANGE_MUST_APPEAR_IN_DIFF";
    std::fs::write(
        &abs_working,
        format!("base content\n{uncommitted_marker}\n"),
    )
    .expect("write uncommitted change to working file");

    // Set up fixture with isolated repo
    let workspace = MemoryWorkspace::new_test().with_dir(".agent/tmp");
    let mut fixture = TestFixture::with_workspace(workspace);
    fixture.repo_root = repo_dir.path().to_path_buf();
    let ctx = fixture.ctx();

    MainEffectHandler::check_commit_diff(&ctx)
        .expect("check_commit_diff should succeed with isolated repo");

    let diff = fixture
        .workspace
        .read(Path::new(".agent/tmp/commit_diff.txt"))
        .expect("commit diff file must be written");

    // C (uncommitted) must appear — proves HEAD diff captures working tree changes.
    assert!(
        diff.contains(uncommitted_marker),
        "expected uncommitted change marker in commit diff; got: {diff}"
    );

    // B (committed) must NOT appear — proves HEAD baseline is used, not start_commit.
    assert!(
        !diff.contains(committed_marker),
        "expected already-committed change to be ABSENT from commit diff (HEAD baseline); got: {diff}"
    );
}
