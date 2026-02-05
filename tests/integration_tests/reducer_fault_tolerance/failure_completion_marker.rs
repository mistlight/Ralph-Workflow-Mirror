//! Integration test for completion marker emission on pipeline failure.
//!
//! Verifies that when the pipeline reaches Status: Failed (AgentChainExhausted),
//! it properly:
//! 1. Transitions to AwaitingDevFix phase
//! 2. Triggers TriggerDevFixFlow effect
//! 3. Emits completion marker to filesystem
//! 4. Transitions to Interrupted phase
//! 5. Saves checkpoint (making is_complete() return true)

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::{AgentRegistry, AgentRole};
use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
use ralph_workflow::checkpoint::{ExecutionHistory, RunContext};
use ralph_workflow::config::Config;
use ralph_workflow::executor::MockProcessExecutor;
use ralph_workflow::logger::{Colors, Logger};
use ralph_workflow::pipeline::{Stats, Timer};
use ralph_workflow::prompts::template_context::TemplateContext;
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::{Effect, EffectResult};
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::handler::MainEffectHandler;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{io, panic};

struct Fixture {
    config: Config,
    colors: Colors,
    logger: Logger,
    timer: Timer,
    stats: Stats,
    template_context: TemplateContext,
    registry: AgentRegistry,
    executor: Arc<MockProcessExecutor>,
    repo_root: PathBuf,
    workspace: Arc<dyn Workspace>,
}

impl Fixture {
    fn new() -> Self {
        let repo_root = PathBuf::from("/test/repo");
        let workspace: Arc<dyn Workspace> = Arc::new(MemoryWorkspace::new(repo_root.clone()));
        Self::with_workspace(workspace)
    }

    fn with_workspace(workspace: Arc<dyn Workspace>) -> Self {
        let config = Config::default();
        let colors = Colors::new();
        let repo_root = workspace.root().to_path_buf();
        let logger = Logger::new(colors);
        let registry = AgentRegistry::new().unwrap();
        let executor = Arc::new(MockProcessExecutor::new());

        Self {
            config,
            colors,
            logger,
            timer: Timer::new(),
            stats: Stats::default(),
            template_context: TemplateContext::default(),
            registry,
            executor,
            repo_root,
            workspace,
        }
    }

    fn ctx(&mut self) -> ralph_workflow::phases::PhaseContext<'_> {
        ralph_workflow::phases::PhaseContext {
            config: &self.config,
            registry: &self.registry,
            logger: &self.logger,
            colors: &self.colors,
            timer: &mut self.timer,
            stats: &mut self.stats,
            developer_agent: "test-developer",
            reviewer_agent: "test-reviewer",
            review_guidelines: None,
            template_context: &self.template_context,
            run_context: RunContext::new(),
            execution_history: ExecutionHistory::new(),
            prompt_history: std::collections::HashMap::new(),
            executor: &*self.executor,
            executor_arc: Arc::clone(&self.executor)
                as Arc<dyn ralph_workflow::executor::ProcessExecutor>,
            repo_root: &self.repo_root,
            workspace: self.workspace.as_ref(),
        }
    }
}

#[derive(Debug)]
struct FailingWorkspace {
    inner: MemoryWorkspace,
    fail_marker_write: bool,
}

impl FailingWorkspace {
    fn new(inner: MemoryWorkspace, fail_marker_write: bool) -> Self {
        Self {
            inner,
            fail_marker_write,
        }
    }

    fn should_fail_marker_write(&self, path: &Path) -> bool {
        self.fail_marker_write && path == Path::new(".agent/tmp/completion_marker")
    }
}

impl Workspace for FailingWorkspace {
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn read(&self, relative: &Path) -> io::Result<String> {
        self.inner.read(relative)
    }

    fn read_bytes(&self, relative: &Path) -> io::Result<Vec<u8>> {
        self.inner.read_bytes(relative)
    }

    fn write(&self, relative: &Path, content: &str) -> io::Result<()> {
        if self.should_fail_marker_write(relative) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated completion marker write failure",
            ));
        }
        self.inner.write(relative, content)
    }

    fn write_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        if self.should_fail_marker_write(relative) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated completion marker write failure",
            ));
        }
        self.inner.write_bytes(relative, content)
    }

    fn append_bytes(&self, relative: &Path, content: &[u8]) -> io::Result<()> {
        self.inner.append_bytes(relative, content)
    }

    fn exists(&self, relative: &Path) -> bool {
        self.inner.exists(relative)
    }

    fn is_file(&self, relative: &Path) -> bool {
        self.inner.is_file(relative)
    }

    fn is_dir(&self, relative: &Path) -> bool {
        self.inner.is_dir(relative)
    }

    fn remove(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove(relative)
    }

    fn remove_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_if_exists(relative)
    }

    fn remove_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(relative)
    }

    fn remove_dir_all_if_exists(&self, relative: &Path) -> io::Result<()> {
        self.inner.remove_dir_all_if_exists(relative)
    }

    fn create_dir_all(&self, relative: &Path) -> io::Result<()> {
        self.inner.create_dir_all(relative)
    }

    fn read_dir(&self, relative: &Path) -> io::Result<Vec<ralph_workflow::workspace::DirEntry>> {
        self.inner.read_dir(relative)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }

    fn write_atomic(&self, relative: &Path, content: &str) -> io::Result<()> {
        if self.should_fail_marker_write(relative) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "simulated completion marker write failure",
            ));
        }
        self.inner.write_atomic(relative, content)
    }

    fn set_readonly(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_readonly(relative)
    }

    fn set_writable(&self, relative: &Path) -> io::Result<()> {
        self.inner.set_writable(relative)
    }
}

#[derive(Debug, Clone, Copy)]
enum SaveBehavior {
    Ok,
    ErrorEvent,
    Panic,
}

#[derive(Debug)]
struct StalledAwaitingDevFixHandler {
    state: PipelineState,
    save_behavior: SaveBehavior,
    save_attempts: usize,
}

impl StalledAwaitingDevFixHandler {
    fn new(state: PipelineState, save_behavior: SaveBehavior) -> Self {
        Self {
            state,
            save_behavior,
            save_attempts: 0,
        }
    }
}

impl<'ctx> ralph_workflow::reducer::effect::EffectHandler<'ctx> for StalledAwaitingDevFixHandler {
    fn execute(
        &mut self,
        effect: Effect,
        _ctx: &mut ralph_workflow::phases::PhaseContext<'_>,
    ) -> anyhow::Result<EffectResult> {
        match effect {
            Effect::TriggerDevFixFlow {
                failed_phase,
                failed_role,
                ..
            } => Ok(EffectResult::event(PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                    failed_phase,
                    failed_role,
                },
            ))),
            Effect::SaveCheckpoint { trigger } => {
                self.save_attempts += 1;
                match self.save_behavior {
                    SaveBehavior::Ok => Ok(EffectResult::event(PipelineEvent::checkpoint_saved(
                        trigger,
                    ))),
                    SaveBehavior::ErrorEvent => Err(ErrorEvent::WorkspaceWriteFailed {
                        path: ".agent/checkpoint.json".to_string(),
                        kind: ralph_workflow::reducer::event::WorkspaceIoErrorKind::Other,
                    }
                    .into()),
                    SaveBehavior::Panic => panic!("simulated SaveCheckpoint panic"),
                }
            }
            other => Err(anyhow::anyhow!("unexpected effect: {other:?}")),
        }
    }
}

impl ralph_workflow::app::event_loop::StatefulHandler for StalledAwaitingDevFixHandler {
    fn update_state(&mut self, state: PipelineState) {
        self.state = state;
    }
}

#[test]
fn test_agent_chain_exhausted_emits_completion_marker() {
    with_default_timeout(|| {
        // Given: Initial pipeline state
        let state = PipelineState::initial(1, 1);
        assert_eq!(state.phase, PipelinePhase::Planning);

        // When: AgentChainExhausted error occurs
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: State transitions to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));

        // When: Orchestration determines next effect
        let effect = determine_next_effect(&new_state);

        // Then: Effect should be TriggerDevFixFlow
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "Expected TriggerDevFixFlow, got {:?}",
            effect
        );

        // Verify full event loop execution emits completion marker
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut handler = MockEffectHandler::new(new_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(new_state), config, &mut handler)
            .expect("Event loop should complete");

        // Then: Pipeline should complete
        assert!(
            result.completed,
            "Pipeline should complete after failure handling"
        );

        // Then: Completion marker should exist in workspace
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker file should exist"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure, got: {}",
            marker_content
        );
    });
}

#[test]
fn test_failed_status_dispatches_dev_fix_agent_and_emits_completion_marker() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        fixture
            .workspace
            .write(Path::new("PROMPT.md"), "Fix pipeline failure")
            .expect("PROMPT.md should be writable");
        fixture
            .workspace
            .write(
                Path::new(".agent/PLAN.md"),
                "1. Diagnose failure\n2. Fix root cause",
            )
            .expect("PLAN.md should be writable");

        let mut ctx = fixture.ctx();

        let mut state = PipelineState {
            phase: PipelinePhase::AwaitingDevFix,
            previous_phase: Some(PipelinePhase::Development),
            ..PipelineState::initial(1, 1)
        };
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let mut handler = MainEffectHandler::new(state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should complete");

        assert!(result.completed, "Failure handling should complete");
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker should be written"
        );
        assert!(
            !fixture.executor.agent_calls().is_empty(),
            "Dev-fix agent should be dispatched on failure"
        );
    });
}

#[test]
fn test_failure_status_triggers_awaiting_dev_fix_not_immediate_exit() {
    with_default_timeout(|| {
        // Given: Pipeline in Development phase
        let state = PipelineState::initial(2, 1);

        // When: AgentChainExhausted occurs during Development
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 5,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: Should transition to AwaitingDevFix, NOT Interrupted
        assert_eq!(
            new_state.phase,
            PipelinePhase::AwaitingDevFix,
            "Should enter AwaitingDevFix phase for remediation attempt"
        );

        // And: Should NOT be complete yet (needs to process dev-fix flow)
        assert!(
            !new_state.is_complete(),
            "Should not be complete in AwaitingDevFix phase"
        );

        // When: TriggerDevFixFlow effect is processed (simulated)
        let after_trigger_state = reduce(
            new_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixTriggered {
                    failed_phase: PipelinePhase::Development,
                    failed_role: AgentRole::Developer,
                },
            ),
        );

        let after_fix_state = reduce(
            after_trigger_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixCompleted {
                    success: false,
                    summary: None,
                },
            ),
        );

        // When: CompletionMarkerEmitted event is processed
        let interrupted_state = reduce(
            after_fix_state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Then: Should be in Interrupted phase
        assert_eq!(interrupted_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            interrupted_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );

        // And: Next effect should be SaveCheckpoint
        let next_effect = determine_next_effect(&interrupted_state);
        assert!(
            matches!(next_effect, Effect::SaveCheckpoint { .. }),
            "Expected SaveCheckpoint for Interrupted phase, got {:?}",
            next_effect
        );
    });
}

#[test]
fn test_completion_marker_written_before_interrupted_transition() {
    with_default_timeout(|| {
        // This test verifies the completion marker is written DURING TriggerDevFixFlow
        // effect execution, not after transitioning to Interrupted

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let state = PipelineState::initial(1, 1);

        // Transition to AwaitingDevFix
        let awaiting_fix_state = reduce(
            state,
            PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
                phase: PipelinePhase::Planning,
                error: ErrorEvent::AgentChainExhausted {
                    role: AgentRole::Developer,
                    phase: PipelinePhase::Planning,
                    cycle: 1,
                },
            }),
        );

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig { max_iterations: 50 };

        let _result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should complete");

        // Verify completion marker exists and contains failure information
        let marker_path = Path::new(".agent/tmp/completion_marker");
        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Completion marker should exist");

        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );
        assert!(
            marker_content.contains("Agent chain exhausted") || marker_content.contains("phase="),
            "Completion marker should include failure details"
        );
    });
}

#[test]
fn test_failure_completion_full_event_loop_with_logging() {
    with_default_timeout(|| {
        // This test verifies that AgentChainExhausted triggers the complete
        // failure handling flow through the event loop, emitting completion marker
        // and completing successfully WITHOUT triggering the defensive completion marker.
        //
        // Expected flow:
        // 1. AgentChainExhausted error -> AwaitingDevFix phase
        // 2. TriggerDevFixFlow effect -> writes completion marker + emits events
        // 3. CompletionMarkerEmitted event -> Interrupted phase
        // 4. SaveCheckpoint effect -> CheckpointSaved event
        // 5. is_complete() returns true
        // 6. Event loop exits with completed=true

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start in AwaitingDevFix phase (simulating an AgentChainExhausted error)
        let state = PipelineState {
            phase: PipelinePhase::AwaitingDevFix,
            previous_phase: Some(PipelinePhase::Development),
            ..PipelineState::initial(2, 1)
        };

        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        // Verify completion marker was written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker should be written during TriggerDevFixFlow"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure, got: {}",
            marker_content
        );

        // CRITICAL: Event loop should complete successfully
        assert!(
            result.completed,
            "Event loop MUST complete after failure handling. \
             If this fails, the 'Pipeline exited without completion marker' bug has occurred. \
             Check event loop logs for: phase, checkpoint_saved_count, exit reason."
        );

        // Verify we processed the expected events:
        // TriggerDevFixFlow -> DevFixTriggered + DevFixCompleted + CompletionMarkerEmitted (3 events)
        // SaveCheckpoint -> CheckpointSaved (1 event)
        // Total: at least 4 events
        assert!(
            result.events_processed >= 4,
            "Should process at least 4 events (DevFixTriggered, DevFixCompleted, CompletionMarkerEmitted, CheckpointSaved), got {}",
            result.events_processed
        );
    });
}

#[test]
fn test_event_loop_does_not_exit_prematurely_on_agent_exhaustion() {
    with_default_timeout(|| {
        // This test specifically targets the bug where the event loop exits
        // with completed=false when AgentChainExhausted occurs.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start in Planning phase (will transition to AwaitingDevFix on error)
        let state = PipelineState::initial(1, 1);

        // Inject AgentChainExhausted error
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        let awaiting_fix_state = reduce(state, error_event);
        assert_eq!(awaiting_fix_state.phase, PipelinePhase::AwaitingDevFix);

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should not error");

        // CRITICAL: Event loop MUST report completion
        assert!(
            result.completed,
            "BUG: Event loop exited without completion marker. \
             This is the bug we're fixing. \
             final_phase={:?}, events_processed={}, checkpoint_saved_count={}",
            result.final_phase, result.events_processed, handler.state.checkpoint_saved_count
        );

        // Verify we reached Interrupted phase with checkpoint saved
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after failure handling"
        );

        // Verify completion marker exists
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker must be written during dev-fix flow"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure"
        );
    });
}

#[test]
fn test_max_iterations_in_awaiting_dev_fix_emits_completion_marker() {
    with_default_timeout(|| {
        // This test validates the defensive completion marker logic when max iterations
        // is reached while in AwaitingDevFix phase. This is the specific bug fix for:
        // "Pipeline exited without completion marker" when max iterations hit during dev-fix.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Create a state in AwaitingDevFix phase
        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler = StalledAwaitingDevFixHandler::new(state.clone(), SaveBehavior::Ok);
        // Set a low max_iterations to trigger the defensive logic
        let max_iterations = 5;
        let config = EventLoopConfig { max_iterations };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        // CRITICAL: Even when hitting max iterations in AwaitingDevFix,
        // the event loop MUST report completion after writing the marker
        assert!(
            result.completed,
            "BUG: Event loop hit max iterations in AwaitingDevFix and exited without completion. \
             The defensive completion marker logic should have forced completion. \
             final_phase={:?}, events_processed={}, checkpoint_saved_count={}",
            result.final_phase, result.events_processed, handler.state.checkpoint_saved_count
        );

        // Verify we transitioned to Interrupted
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should have forced transition to Interrupted when max iterations hit in AwaitingDevFix"
        );

        // Verify checkpoint_saved_count was incremented to satisfy is_complete()
        assert!(
            handler.state.checkpoint_saved_count > 0,
            "checkpoint_saved_count should be incremented after forced completion"
        );

        assert!(
            handler.save_attempts > 0,
            "SaveCheckpoint should be attempted after forced completion"
        );

        // Verify completion marker was written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker must be written when max iterations hit in AwaitingDevFix"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );
        assert!(
            result.events_processed >= max_iterations,
            "Event loop should reach max iterations to exercise forced completion"
        );
    });
}

#[test]
fn test_forced_completion_transitions_to_interrupted_when_marker_write_fails() {
    with_default_timeout(|| {
        let failing_workspace = Arc::new(FailingWorkspace::new(MemoryWorkspace::new_test(), true));
        let mut fixture = Fixture::with_workspace(failing_workspace);
        let mut ctx = fixture.ctx();

        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler = StalledAwaitingDevFixHandler::new(state.clone(), SaveBehavior::Ok);
        let config = EventLoopConfig { max_iterations: 3 };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        assert!(
            result.completed,
            "Forced completion should mark the event loop as complete"
        );
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Forced completion should transition to Interrupted even if marker write fails"
        );
        assert!(
            handler.save_attempts > 0,
            "SaveCheckpoint should be attempted even if marker write fails"
        );

        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            !fixture.workspace.exists(marker_path),
            "Completion marker should not exist when write fails"
        );
    });
}

#[test]
fn test_forced_completion_catches_save_checkpoint_panic() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler = StalledAwaitingDevFixHandler::new(state.clone(), SaveBehavior::Panic);
        let config = EventLoopConfig { max_iterations: 3 };

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
        }));

        assert!(
            result.is_ok(),
            "SaveCheckpoint panic should be caught by event loop"
        );

        let loop_result = result.expect("Expected event loop result");
        assert!(
            loop_result.is_ok(),
            "Event loop should return Ok result when handling panics"
        );

        let loop_result = loop_result.expect("Expected event loop result");
        assert!(
            !loop_result.completed,
            "Event loop should report incomplete when SaveCheckpoint panics"
        );
    });
}

#[test]
fn test_forced_completion_reduces_save_checkpoint_error_event() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler =
            StalledAwaitingDevFixHandler::new(state.clone(), SaveBehavior::ErrorEvent);
        let max_iterations = 3;
        let config = EventLoopConfig { max_iterations };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        assert!(
            result.events_processed >= max_iterations,
            "Event loop should hit max iterations to exercise forced completion"
        );
        assert_eq!(
            handler.state.previous_phase,
            Some(PipelinePhase::Interrupted),
            "SaveCheckpoint error event should be reduced through the reducer"
        );
    });
}

#[test]
fn test_interrupted_from_dev_fix_is_complete_before_checkpoint() {
    with_default_timeout(|| {
        // This test validates the fix for the "Pipeline exited without completion marker" bug.
        // It verifies that when transitioning from AwaitingDevFix to Interrupted,
        // is_complete() returns true even before SaveCheckpoint executes.

        let mut state = PipelineState::initial(1, 1);

        // Simulate the transition path: Planning → AwaitingDevFix → Interrupted
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);

        // After TriggerDevFixFlow completes and CompletionMarkerEmitted event is processed
        let after_marker_state = reduce(
            state,
            PipelineEvent::AwaitingDevFix(
                ralph_workflow::reducer::event::AwaitingDevFixEvent::CompletionMarkerEmitted {
                    is_failure: true,
                },
            ),
        );

        // Verify state transitioned to Interrupted
        assert_eq!(after_marker_state.phase, PipelinePhase::Interrupted);
        assert_eq!(
            after_marker_state.previous_phase,
            Some(PipelinePhase::AwaitingDevFix)
        );
        assert_eq!(after_marker_state.checkpoint_saved_count, 0);

        // CRITICAL: is_complete() should return true even without checkpoint
        // because we came from AwaitingDevFix (completion marker already written)
        assert!(
            after_marker_state.is_complete(),
            "BUG: is_complete() should return true for Interrupted phase from AwaitingDevFix, \
             even without checkpoint, because completion marker was already written. \
             This is the fix for 'Pipeline exited without completion marker'."
        );

        // Verify next effect is SaveCheckpoint
        let next_effect = determine_next_effect(&after_marker_state);
        assert!(
            matches!(next_effect, Effect::SaveCheckpoint { .. }),
            "Next effect should be SaveCheckpoint, got {:?}",
            next_effect
        );
    });
}

#[test]
fn test_awaiting_dev_fix_executes_trigger_before_max_iterations() {
    with_default_timeout(|| {
        // This test verifies the fix for the bug where the event loop could exit
        // from AwaitingDevFix without executing TriggerDevFixFlow when approaching
        // max iterations.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Create a state that transitions to AwaitingDevFix after several iterations
        let mut state = PipelineState::initial(1, 1);

        // Simulate AgentChainExhausted error
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        state = reduce(state, error_event);
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert!(
            !state.dev_fix_triggered,
            "dev_fix_triggered should start false"
        );

        // Set a low max_iterations to simulate approaching the limit
        // With the bug, the loop would exit here without executing TriggerDevFixFlow
        // With the fix, TriggerDevFixFlow should execute before completion check
        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig { max_iterations: 10 };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should complete successfully");

        // Verify TriggerDevFixFlow executed
        assert!(
            result.completed,
            "Event loop should complete after executing TriggerDevFixFlow"
        );
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after dev-fix flow"
        );

        // Verify completion marker was written
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker should be written even when approaching max iterations"
        );

        // Verify dev_fix_triggered flag was set
        assert!(
            handler.state.dev_fix_triggered,
            "dev_fix_triggered flag should be set after TriggerDevFixFlow executes"
        );
    });
}

#[test]
fn test_budget_exhaustion_continues_to_commit_not_terminate() {
    with_default_timeout(|| {
        // This test verifies that when dev-fix budget is exhausted (simulated by
        // hitting max iterations in AwaitingDevFix), the pipeline advances to
        // commit/finalization phase instead of terminating early.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler = StalledAwaitingDevFixHandler::new(state.clone(), SaveBehavior::Ok);
        let config = EventLoopConfig { max_iterations: 5 };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should complete");

        // Pipeline should complete (transition to Interrupted, then checkpoint saved)
        assert!(
            result.completed,
            "Pipeline must complete after budget exhaustion, not terminate early"
        );

        // Verify completion marker was written
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker must be written even when budget exhausted"
        );

        // Verify we're in Interrupted phase (ready for commit/finalization)
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Budget exhaustion should transition to Interrupted for commit/finalization"
        );
    });
}

#[test]
fn test_regression_pipeline_exits_without_completion_marker_on_dev_iter_2_failure() {
    with_default_timeout(|| {
        // Regression test for: "Pipeline exited without completion marker"
        // Scenario: Development Iteration 2 fails, Status: Failed, pipeline should
        // continue via dev-fix flow (or commit if budget exhausted), not exit early.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start at Development iteration 2 (simulating the bug report scenario)
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 2;

        // Simulate AgentChainExhausted during Development
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let awaiting_fix_state = reduce(state, error_event);
        assert_eq!(
            awaiting_fix_state.phase,
            PipelinePhase::AwaitingDevFix,
            "AgentChainExhausted should transition to AwaitingDevFix"
        );

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should not error");

        // CRITICAL: Pipeline must complete, not exit early
        assert!(
            result.completed,
            "REGRESSION: Pipeline exited without completion. \
             This is the original bug. \
             Status: Failed should trigger dev-fix flow, not immediate exit. \
             final_phase={:?}, events_processed={}",
            result.final_phase, result.events_processed
        );

        // Verify completion marker was written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "REGRESSION: Completion marker missing. Original bug reproduced."
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );

        // Verify we transitioned to Interrupted (ready for commit/finalization)
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after dev-fix flow"
        );
    });
}
