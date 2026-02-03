//! Prompt dispatch functions.
//!
//! Contains the main dispatcher functions for routing to appropriate prompt generators
//! based on role and action, as well as prompt replay functionality for checkpoint resume.

use super::prompt_config::PromptConfig;
use super::resume_note::generate_resume_note;
use super::types::{Action, Role};
use super::ContextLevel;
use super::TemplateContext;
use super::{
    prompt_developer_iteration_with_context, prompt_fix_with_context, prompt_plan_with_context,
};

/// Generate a prompt for any agent type.
///
/// This is the main dispatcher function that routes to the appropriate
/// prompt generator based on role and action.
///
/// The config parameter allows providing:
/// - Language-specific review guidance when the project stack has been detected
/// - PROMPT.md content for planning prompts
/// - PROMPT.md and PLAN.md content for developer iteration prompts
///
/// # Arguments
///
/// * `role` - The agent role (Developer, Reviewer, etc.)
/// * `action` - The action to perform (Plan, Iterate, Fix, etc.)
/// * `context` - The context level (minimal or normal)
/// * `template_context` - Template context for user template overrides
/// * `config` - Prompt configuration with content variables
pub fn prompt_for_agent(
    role: Role,
    action: Action,
    context: ContextLevel,
    template_context: &TemplateContext,
    config: PromptConfig,
) -> String {
    let resume_note = if let Some(resume_ctx) = &config.resume_context {
        generate_resume_note(resume_ctx)
    } else if config.is_resume {
        // Fallback when no rich ResumeContext is available (uses simpler note)
        "\nNOTE: This session is resuming from a previous run. Previous progress is preserved in git history.\n\n".to_string()
    } else {
        String::new()
    };

    let base_prompt = match (role, action) {
        (_, Action::Plan) => {
            prompt_plan_with_context(template_context, config.prompt_md_content.as_deref())
        }
        (Role::Developer | Role::Reviewer, Action::Iterate) => {
            let (prompt_content, plan_content) = config
                .prompt_and_plan
                .unwrap_or((String::new(), String::new()));
            prompt_developer_iteration_with_context(
                template_context,
                config.iteration.unwrap_or(1),
                config.total_iterations.unwrap_or(1),
                context,
                &prompt_content,
                &plan_content,
            )
        }
        (_, Action::Fix) => {
            let (prompt_content, plan_content, issues_content) = config
                .prompt_plan_and_issues
                .unwrap_or((String::new(), String::new(), String::new()));
            prompt_fix_with_context(
                template_context,
                &prompt_content,
                &plan_content,
                &issues_content,
            )
        }
    };

    // Prepend resume note if applicable
    if config.is_resume {
        format!("{}{}", resume_note, base_prompt)
    } else {
        base_prompt
    }
}

/// Get a stored prompt from history or generate a new one.
///
/// This function implements prompt replay for hardened resume functionality.
/// When resuming from a checkpoint, it checks if a prompt was already used
/// and returns the stored prompt for deterministic behavior. Otherwise, it
/// generates a new prompt using the provided generator function.
///
/// # Arguments
///
/// * `prompt_key` - Unique key identifying this prompt (e.g., "development_1", "review_2")
/// * `prompt_history` - The prompt history from the checkpoint (if available)
/// * `generator` - Function to generate the prompt if not found in history
///
/// # Returns
///
/// A tuple of (prompt, was_replayed) where:
/// - `prompt` is the prompt string (either replayed or newly generated)
/// - `was_replayed` is true if the prompt came from history, false if newly generated
///
/// # Example
///
/// ```ignore
/// let (prompt, was_replayed) = get_stored_or_generate_prompt(
///     "development_1",
///     &ctx.prompt_history,
///     || prompt_for_agent(role, action, context, template_context, config),
/// );
/// if was_replayed {
///     logger.info("Using stored prompt from checkpoint for determinism");
/// }
/// ```
pub fn get_stored_or_generate_prompt<F>(
    prompt_key: &str,
    prompt_history: &std::collections::HashMap<String, String>,
    generator: F,
) -> (String, bool)
where
    F: FnOnce() -> String,
{
    if let Some(stored_prompt) = prompt_history.get(prompt_key) {
        (stored_prompt.clone(), true)
    } else {
        (generator(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stored_or_generate_prompt_replays_when_available() {
        let mut history = std::collections::HashMap::new();
        history.insert("test_key".to_string(), "stored prompt".to_string());

        let (prompt, was_replayed) =
            get_stored_or_generate_prompt("test_key", &history, || "generated prompt".to_string());

        assert_eq!(prompt, "stored prompt");
        assert!(was_replayed, "Should have replayed the stored prompt");
    }

    #[test]
    fn test_get_stored_or_generate_prompt_generates_when_not_available() {
        let history = std::collections::HashMap::new();

        let (prompt, was_replayed) = get_stored_or_generate_prompt("missing_key", &history, || {
            "generated prompt".to_string()
        });

        assert_eq!(prompt, "generated prompt");
        assert!(!was_replayed, "Should have generated a new prompt");
    }

    #[test]
    fn test_get_stored_or_generate_prompt_with_empty_history() {
        let history = std::collections::HashMap::new();

        let (prompt, was_replayed) =
            get_stored_or_generate_prompt("any_key", &history, || "fresh prompt".to_string());

        assert_eq!(prompt, "fresh prompt");
        assert!(
            !was_replayed,
            "Should have generated a new prompt for empty history"
        );
    }
}
