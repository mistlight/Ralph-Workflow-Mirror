// Unit tests for runner module.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::{AppEffect, AppEffectHandler, AppEffectResult};

    #[derive(Debug)]
    struct TestRepoRootHandler {
        captured: Vec<AppEffect>,
        repo_root: std::path::PathBuf,
    }

    impl TestRepoRootHandler {
        fn new(repo_root: std::path::PathBuf) -> Self {
            Self {
                captured: Vec::new(),
                repo_root,
            }
        }
    }

    impl AppEffectHandler for TestRepoRootHandler {
        fn execute(&mut self, effect: AppEffect) -> AppEffectResult {
            self.captured.push(effect.clone());
            match effect {
                AppEffect::SetCurrentDir { .. } | AppEffect::GitRequireRepo => AppEffectResult::Ok,
                AppEffect::GitGetRepoRoot => AppEffectResult::Path(self.repo_root.clone()),
                other => panic!("unexpected effect in test handler: {other:?}"),
            }
        }
    }

    #[test]
    fn discover_repo_root_for_workspace_prefers_git_repo_root_over_override_dir() {
        let override_dir = std::path::PathBuf::from("/override/subdir");
        let repo_root = std::path::PathBuf::from("/repo");
        let mut handler = TestRepoRootHandler::new(repo_root.clone());

        let got = discover_repo_root_for_workspace(Some(&override_dir), &mut handler).unwrap();
        assert_eq!(got, repo_root);

        assert!(matches!(
            handler.captured.first(),
            Some(AppEffect::SetCurrentDir { .. })
        ));
        assert!(handler
            .captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitRequireRepo)));
        assert!(handler
            .captured
            .iter()
            .any(|e| matches!(e, AppEffect::GitGetRepoRoot)));
    }
}
