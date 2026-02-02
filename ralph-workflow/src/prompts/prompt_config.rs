//! Prompt configuration types.
//!
//! Groups related parameters for prompt generation to reduce function argument count.
//!
//! This module is only available in test builds and when the `test-utils` feature is enabled.

use crate::checkpoint::restore::ResumeContext;

/// Configuration for prompt generation.
///
/// Groups related parameters to reduce function argument count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[must_use]
pub struct PromptConfig {
    /// The current iteration number (for developer iteration prompts).
    pub iteration: Option<u32>,
    /// The total number of iterations (for developer iteration prompts).
    pub total_iterations: Option<u32>,
    /// PROMPT.md content for planning prompts.
    pub prompt_md_content: Option<String>,
    /// (PROMPT.md, PLAN.md) content tuple for developer iteration prompts.
    pub prompt_and_plan: Option<(String, String)>,
    /// (PROMPT.md, PLAN.md, ISSUES.md) content tuple for fix prompts.
    pub prompt_plan_and_issues: Option<(String, String, String)>,
    /// Whether this is a resumed session (from a checkpoint).
    pub is_resume: bool,
    /// Rich resume context if available.
    pub resume_context: Option<ResumeContext>,
}

impl PromptConfig {
    /// Create a new prompt configuration with default values.
    #[must_use = "configuration is required for prompt generation"]
    pub const fn new() -> Self {
        Self {
            iteration: None,
            total_iterations: None,
            prompt_md_content: None,
            prompt_and_plan: None,
            prompt_plan_and_issues: None,
            is_resume: false,
            resume_context: None,
        }
    }

    /// Set iteration numbers for developer iteration prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub const fn with_iterations(mut self, iteration: u32, total: u32) -> Self {
        self.iteration = Some(iteration);
        self.total_iterations = Some(total);
        self
    }

    /// Set PROMPT.md content for planning prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_prompt_md(mut self, content: String) -> Self {
        self.prompt_md_content = Some(content);
        self
    }

    /// Set (PROMPT.md, PLAN.md) content tuple for developer iteration prompts.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_prompt_and_plan(mut self, prompt: String, plan: String) -> Self {
        self.prompt_and_plan = Some((prompt, plan));
        self
    }

    /// Set (PROMPT.md, PLAN.md, ISSUES.md) content tuple for fix prompts.
    pub fn with_prompt_plan_and_issues(
        mut self,
        prompt: String,
        plan: String,
        issues: String,
    ) -> Self {
        self.prompt_plan_and_issues = Some((prompt, plan, issues));
        self
    }

    /// Set whether this is a resumed session.
    #[cfg(test)]
    #[must_use = "returns the updated configuration for chaining"]
    pub const fn with_resume(mut self, is_resume: bool) -> Self {
        self.is_resume = is_resume;
        self
    }

    /// Set rich resume context for resumed sessions.
    #[must_use = "returns the updated configuration for chaining"]
    pub fn with_resume_context(mut self, context: ResumeContext) -> Self {
        self.resume_context = Some(context);
        self.is_resume = true;
        self
    }
}
