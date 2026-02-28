use super::super::common::TestFixture;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::reducer::handler::MainEffectHandler;
use crate::reducer::state::{AgentChainState, CommitState, PipelineState};
use crate::workspace::Workspace;
use std::path::Path;

#[test]
fn test_cleanup_commit_xml_removes_stale_commit_xml() {
    let workspace = crate::workspace::MemoryWorkspace::new_test()
        .with_file(".agent/tmp/commit_prompt.txt", "commit prompt")
        .with_file(
            xml_paths::COMMIT_MESSAGE_XML,
            "<ralph-commit-message>old</ralph-commit-message>",
        );
    let mut fixture = TestFixture::with_workspace(workspace);
    let ctx = fixture.ctx();

    let mut handler = MainEffectHandler::new(PipelineState::initial(1, 1));
    handler.state.commit = CommitState::Generating {
        attempt: 1,
        max_attempts: 2,
    };
    handler.state.agent_chain = AgentChainState::initial().with_agents(
        vec!["claude".to_string()],
        vec![vec![]],
        crate::agents::AgentRole::Commit,
    );

    let _ = handler.cleanup_commit_xml(&ctx);

    assert!(
        !fixture
            .workspace
            .exists(Path::new(xml_paths::COMMIT_MESSAGE_XML)),
        "stale commit XML should be cleared before invoking commit agent"
    );
}
