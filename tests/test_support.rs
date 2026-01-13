use git2::{IndexAddOption, Oid, Repository, Signature, Status, StatusOptions};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

pub fn init_git_repo(dir: &TempDir) -> Repository {
    let repo = Repository::init(dir.path()).expect("init git repo");

    // Configure user for libgit2's repo.signature() and Ralph's git_commit().
    let mut cfg = repo.config().expect("repo config");
    cfg.set_str("user.name", "Test User").expect("set user.name");
    cfg.set_str("user.email", "test@example.com")
        .expect("set user.email");

    fs::write(
        dir.path().join(".gitignore"),
        ".agent/\n.no_agent_commit\nPROMPT.md\n",
    )
    .expect("write .gitignore");
    fs::write(dir.path().join("PROMPT.md"), "# Test Requirements\nTest task")
        .expect("write PROMPT.md");
    fs::create_dir_all(dir.path().join(".agent")).expect("create .agent");

    repo
}

pub fn write_file<P: AsRef<Path>>(path: P, contents: &str) {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }
    fs::write(path, contents).expect("write file");
}

pub fn commit_all(repo: &Repository, message: &str) -> Oid {
    stage_all(repo);

    let mut index = repo.index().expect("open index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");

    let sig = Signature::now("Test User", "test@example.com").expect("signature");

    match repo.head() {
        Ok(head) => {
            let parent = head.peel_to_commit().expect("parent commit");
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                .expect("commit")
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .expect("initial commit"),
        Err(e) => panic!("unexpected head error: {e}"),
    }
}

pub fn head_oid(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default()
}

pub fn commit_count(repo: &Repository) -> usize {
    let mut walk = repo.revwalk().expect("revwalk");
    walk.push_head().expect("push head");
    walk.count()
}

pub fn stage_all(repo: &Repository) {
    let mut index = repo.index().expect("open index");

    // Stage deletions explicitly.
    let mut status_opts = StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo.statuses(Some(&mut status_opts)).expect("statuses");
    for entry in statuses.iter() {
        if entry.status().contains(Status::WT_DELETED) {
            if let Some(path) = entry.path() {
                index
                    .remove_path(Path::new(path))
                    .expect("remove deleted path");
            }
        }
    }

    index
        .add_all(["."], IndexAddOption::DEFAULT, None)
        .expect("add_all");
    index.write().expect("write index");
}

