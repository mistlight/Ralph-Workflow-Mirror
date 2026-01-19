//! Phase execution context.
//!
//! This module defines the shared context that is passed to each phase
//! of the pipeline. It contains references to configuration, registry,
//! logging utilities, and runtime state that all phases need access to.

use crate::agents::{AgentRegistry, AgentRole};
use crate::checkpoint::RunContext;
use crate::config::Config;
use crate::guidelines::ReviewGuidelines;
use crate::logger::{Colors, Logger};
use crate::pipeline::Stats;
use crate::pipeline::Timer;
use crate::prompts::template_context::TemplateContext;

/// Shared context for all pipeline phases.
///
/// This struct holds references to all the shared state that phases need
/// to access. It is passed by mutable reference to each phase function.
pub struct PhaseContext<'a> {
    /// Configuration settings for the pipeline.
    pub config: &'a Config,
    /// Agent registry for looking up agent configurations.
    pub registry: &'a AgentRegistry,
    /// Logger for output and diagnostics.
    pub logger: &'a Logger,
    /// Terminal color configuration.
    pub colors: &'a Colors,
    /// Timer for tracking elapsed time.
    pub timer: &'a mut Timer,
    /// Statistics for tracking pipeline progress.
    pub stats: &'a mut Stats,
    /// Name of the developer agent.
    pub developer_agent: &'a str,
    /// Name of the reviewer agent.
    pub reviewer_agent: &'a str,
    /// Review guidelines based on detected project stack.
    pub review_guidelines: Option<&'a ReviewGuidelines>,
    /// Template context for loading user templates.
    pub template_context: &'a TemplateContext,
    /// Run context for tracking execution lineage and state.
    pub run_context: RunContext,
}

impl PhaseContext<'_> {
    /// Record a completed developer iteration.
    pub fn record_developer_iteration(&mut self) {
        self.run_context.record_developer_iteration();
    }

    /// Record a completed reviewer pass.
    pub fn record_reviewer_pass(&mut self) {
        self.run_context.record_reviewer_pass();
    }
}

/// Get the primary commit agent from the registry.
///
/// This function returns the name of the primary commit agent.
/// If a commit-specific agent is configured, it uses that. Otherwise, it falls back
/// to using the reviewer chain (since commit generation is typically done after review).
pub fn get_primary_commit_agent(ctx: &PhaseContext<'_>) -> Option<String> {
    let fallback_config = ctx.registry.fallback_config();

    // First, try to get commit-specific agents
    let commit_agents = fallback_config.get_fallbacks(AgentRole::Commit);
    if !commit_agents.is_empty() {
        // Return the first commit agent as the primary
        return commit_agents.first().cloned();
    }

    // Fallback to using reviewer agents for commit generation
    let reviewer_agents = fallback_config.get_fallbacks(AgentRole::Reviewer);
    if !reviewer_agents.is_empty() {
        return reviewer_agents.first().cloned();
    }

    // Last resort: use the current reviewer agent
    Some(ctx.reviewer_agent.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::logger::{Colors, Logger};
    use crate::pipeline::{Stats, Timer};
    use crate::prompts::template_context::TemplateContext;

    /// Test fixture for creating `PhaseContext` in tests.
    struct TestFixture {
        config: Config,
        colors: Colors,
        logger: Logger,
        timer: Timer,
        stats: Stats,
        template_context: TemplateContext,
    }

    impl TestFixture {
        fn new() -> Self {
            let colors = Colors { enabled: false };
            Self {
                config: Config::default(),
                colors,
                logger: Logger::new(colors),
                timer: Timer::new(),
                stats: Stats::default(),
                template_context: TemplateContext::default(),
            }
        }
    }

    #[test]
    fn test_get_primary_commit_agent_uses_commit_chain_first() {
        let mut registry = AgentRegistry::new().unwrap();

        // Configure a commit chain
        let toml_str = r#"
            [agent_chain]
            commit = ["commit-agent-1", "commit-agent-2"]
            reviewer = ["reviewer-agent"]
            developer = ["developer-agent"]
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
        registry.apply_unified_config(&unified);

        let mut fixture = TestFixture::new();
        let ctx = PhaseContext {
            config: &fixture.config,
            registry: &registry,
            logger: &fixture.logger,
            colors: &fixture.colors,
            timer: &mut fixture.timer,
            stats: &mut fixture.stats,
            developer_agent: "developer-agent",
            reviewer_agent: "reviewer-agent",
            review_guidelines: None,
            template_context: &fixture.template_context,
            run_context: RunContext::new(),
        };

        let result = get_primary_commit_agent(&ctx);
        assert_eq!(
            result,
            Some("commit-agent-1".to_string()),
            "Should use first agent from commit chain when configured"
        );
    }

    #[test]
    fn test_get_primary_commit_agent_falls_back_to_reviewer_chain() {
        let mut registry = AgentRegistry::new().unwrap();

        // Configure reviewer chain but NO commit chain
        let toml_str = r#"
            [agent_chain]
            reviewer = ["reviewer-agent-1", "reviewer-agent-2"]
            developer = ["developer-agent"]
        "#;
        let unified: crate::config::UnifiedConfig = toml::from_str(toml_str).unwrap();
        registry.apply_unified_config(&unified);

        let mut fixture = TestFixture::new();
        let ctx = PhaseContext {
            config: &fixture.config,
            registry: &registry,
            logger: &fixture.logger,
            colors: &fixture.colors,
            timer: &mut fixture.timer,
            stats: &mut fixture.stats,
            developer_agent: "developer-agent",
            reviewer_agent: "reviewer-agent-1",
            review_guidelines: None,
            template_context: &fixture.template_context,
            run_context: RunContext::new(),
        };

        let result = get_primary_commit_agent(&ctx);
        assert_eq!(
            result,
            Some("reviewer-agent-1".to_string()),
            "Should fall back to first agent from reviewer chain when commit chain is not configured"
        );
    }

    #[test]
    fn test_get_primary_commit_agent_uses_context_reviewer_as_last_resort() {
        let registry = AgentRegistry::new().unwrap();
        // Default registry with no custom chains configured

        let mut fixture = TestFixture::new();
        let ctx = PhaseContext {
            config: &fixture.config,
            registry: &registry,
            logger: &fixture.logger,
            colors: &fixture.colors,
            timer: &mut fixture.timer,
            stats: &mut fixture.stats,
            developer_agent: "fallback-developer",
            reviewer_agent: "fallback-reviewer",
            review_guidelines: None,
            template_context: &fixture.template_context,
            run_context: RunContext::new(),
        };

        let result = get_primary_commit_agent(&ctx);

        // When no chains are configured, it should fall back to the context's reviewer_agent
        // OR the default reviewer from the registry (if it has a default)
        // The key point is it should NOT use developer agent
        assert!(
            result.is_some(),
            "Should return Some agent even with no chains configured"
        );

        // Verify it's not using the developer agent
        assert_ne!(
            result.as_deref(),
            Some("fallback-developer"),
            "Should NOT fall back to developer agent - should use reviewer"
        );
    }
}
