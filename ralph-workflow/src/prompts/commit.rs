//! Commit and fix prompts.
//!
//! Prompts for commit message generation and fix actions.

use crate::prompts::partials::get_shared_partials;
use crate::prompts::template_context::TemplateContext;
use crate::prompts::template_engine::Template;
use crate::prompts::{RenderedTemplate, SubstitutionEntry, SubstitutionLog, SubstitutionSource};
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::fmt::Write;

const COMMIT_MESSAGE_XSD_SCHEMA: &str =
    include_str!("../files/llm_output_extraction/commit_message.xsd");

use crate::files::result_extraction::extract_file_paths_from_issues;

/// Generate fix prompt (applies to either role).
///
/// This prompt is agent-agnostic and works with any AI coding assistant.
/// Uses a template-based approach for consistency with review prompts.
///
/// # Agent-Orchestrator Separation
///
/// The fix agent receives ISSUES content (embedded by the orchestrator after extracting
/// from the reviewer's JSON output) and modifies source code files to fix issues.
/// The agent returns structured output (completion status) that the orchestrator
/// captures via JSON logging.
///
/// ISSUES.md is an orchestrator-managed file - the agent should NOT modify it.
/// The orchestrator writes ISSUES.md before invoking the fix agent and may
/// delete it after fix cycles (e.g., in isolation mode).
///
/// # Constraints
///
/// The fix agent is constrained to ONLY work on files mentioned in the ISSUES content.
/// This prevents the agent from exploring the repository and keeps changes
/// focused on the issues identified during review.
///
/// # Arguments
///
/// * `prompt_content` - Content of PROMPT.md for context about the original request
/// * `plan_content` - Content of PLAN.md for context about the implementation plan
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
#[cfg(test)]
pub fn prompt_fix(prompt_content: &str, plan_content: &str, issues_content: &str) -> String {
    use crate::workspace::WorkspaceFs;
    use std::env;

    let workspace = WorkspaceFs::new(env::current_dir().unwrap());
    let partials = get_shared_partials();
    let template_content = include_str!("templates/fix_mode_xml.txt");

    // Extract file paths from ISSUES content to provide explicit list
    let files_to_modify = extract_file_paths_from_issues(issues_content);
    let files_section = format_files_section(&files_to_modify);

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
        (
            "FIX_RESULT_XML_PATH",
            workspace.absolute_str(".agent/tmp/fix_result.xml"),
        ),
        (
            "FIX_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/fix_result.xsd"),
        ),
    ]);
    Template::new(template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            // Embedded fallback template (XML format)
            format!(
                "FIX MODE\n\nRead .agent/ISSUES.md and fix the issues found.\n\nContext:\nPROMPT:\n{prompt_content}\n\nPLAN:\n{plan_content}\n\nOutput format: <ralph-fix-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-fix-result>\n"
            )
        })
}

/// Generate fix prompt using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `prompt_content` - Content of PROMPT.md for context about the original request
/// * `plan_content` - Content of PLAN.md for context about the implementation plan
/// * `issues_content` - Content of ISSUES.md for context about issues to fix
/// * `workspace` - Workspace for resolving absolute paths
pub fn prompt_fix_with_context(
    context: &TemplateContext,
    prompt_content: &str,
    plan_content: &str,
    issues_content: &str,
    workspace: &dyn Workspace,
) -> String {
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("fix_mode_xml")
        .unwrap_or_else(|_| include_str!("templates/fix_mode_xml.txt").to_string());

    // Extract file paths from ISSUES content to provide explicit list
    let files_to_modify = extract_file_paths_from_issues(issues_content);
    let files_section = format_files_section(&files_to_modify);

    let variables = HashMap::from([
        ("PROMPT", prompt_content.to_string()),
        ("PLAN", plan_content.to_string()),
        ("ISSUES", issues_content.to_string()),
        ("FILES_TO_MODIFY", files_section),
        (
            "FIX_RESULT_XML_PATH",
            workspace.absolute_str(".agent/tmp/fix_result.xml"),
        ),
        (
            "FIX_RESULT_XSD_PATH",
            workspace.absolute_str(".agent/tmp/fix_result.xsd"),
        ),
    ]);
    Template::new(&template_content)
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            // Embedded fallback template (XML format)
            format!(
                "FIX MODE\n\nRead .agent/ISSUES.md and fix the issues found.\n\nContext:\nPROMPT:\n{prompt_content}\n\nPLAN:\n{plan_content}\n\nOutput format: <ralph-fix-result><ralph-status>completed|partial|failed</ralph-status><ralph-summary>Summary</ralph-summary></ralph-fix-result>\n"
            )
        })
}

/// Format the files section for the fix prompt.
///
/// If files are found, formats them as a bulleted list with a clear header.
/// If no files are found, provides a fallback message indicating that the
/// agent may work on any files in the repository to fix the issues.
fn format_files_section(files: &[String]) -> String {
    if files.is_empty() {
        "================================================================================
FILES YOU MAY MODIFY
================================================================================

(No specific files were extracted from ISSUES content)

PERMISSIONS: FULL AUTO MODE - You may work on ANY files in the repository

You are authorized to modify any files in the repository that are needed to fix
the issues described in the ISSUES content above. Use your judgment to determine
which files need modification - you are not limited to files mentioned in ISSUES.

The ISSUES content is already embedded in this prompt - review it carefully.

================================================================================
END OF FILES SECTION
================================================================================
"
        .to_string()
    } else {
        let mut result = String::from(
            "================================================================================
FILES YOU MAY MODIFY
================================================================================

",
        );
        for file in files {
            result.push_str("- ");
            result.push_str(file);
            result.push('\n');
        }
        // Add explicit clarification that agent doesn't need to read any ISSUES file
        result.push_str(
            "
IMPORTANT: Work ONLY with the files listed above. The issues
content is already embedded in this prompt - you do NOT need to
read or discover any files to know what to fix.

================================================================================
END OF FILES SECTION
================================================================================
",
        );
        result
    }
}

/// Generate prompt for creating commit message from provided diff.
///
/// This is used by the orchestrator (not agents) to generate commit messages.
/// The diff is provided directly in the prompt, so the LLM doesn't need to
/// run git commands or access files.
///
/// Uses the XML-based template format for output, which is more reliable than JSON
/// because:
/// - No escape sequence issues (actual newlines work fine in XML)
/// - Distinctive tags (`<ralph-commit>`) unlikely to appear in LLM analysis
/// - Clear boundaries for parsing
///
/// # Arguments
///
/// * `diff` - The git diff to generate a commit message for. If empty or
///   whitespace-only, the prompt will indicate no changes were detected.
///
/// # Note
///
/// This function includes a defensive check for empty diffs - if an empty diff
/// is passed, it returns an error prompt. Callers should check for meaningful
/// changes before calling this function to avoid wasting LLM API calls.
/// The `generate_commit_message` function in phases/commit.rs handles empty
/// diffs by returning the hardcoded fallback commit message.
#[cfg(test)]
pub fn prompt_generate_commit_message_with_diff(diff: &str) -> String {
    use crate::workspace::WorkspaceFs;
    use std::env;

    let workspace = WorkspaceFs::new(env::current_dir().unwrap());
    // Check if diff is empty or whitespace-only
    let diff_content = diff.trim();
    let has_changes = !diff_content.is_empty();

    if !has_changes {
        // Return an error message instead of a placeholder.
        // Callers should check for empty diffs before calling this function.
        // The generate_commit_message function in phases/commit.rs handles this case.
        return "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }

    let template_content = include_str!("templates/commit_message_xml.txt");
    let template = Template::new(template_content);
    let partials = get_shared_partials();
    let variables = HashMap::from([
        ("DIFF", diff_content.to_string()),
        (
            "COMMIT_MESSAGE_XML_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xml"),
        ),
        (
            "COMMIT_MESSAGE_XSD_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render commit template: {e}");
        // Last resort: simple inline prompt (no fallback template needed)
        format!(
            "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
        )
    })
}

/// Generate prompt for commit message from diff with substitution log.
///
/// This is the new log-based version that returns both content and substitution tracking.
/// Use this version in handlers to enable log-based validation.
pub fn prompt_generate_commit_message_with_diff_with_log(
    context: &TemplateContext,
    diff: &str,
    workspace: &dyn Workspace,
    template_name: &str,
) -> RenderedTemplate {
    // Ensure the commit XSD schema is available on disk for agents to reference.
    let tmp_dir = std::path::Path::new(".agent/tmp");
    let _ = workspace.create_dir_all(tmp_dir);
    let _ = workspace.write(
        &tmp_dir.join("commit_message.xsd"),
        COMMIT_MESSAGE_XSD_SCHEMA,
    );

    // Check if diff is empty or whitespace-only
    let diff_content = diff.trim();
    let has_changes = !diff_content.is_empty();

    if !has_changes {
        let prompt_content = "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
        return RenderedTemplate {
            content: prompt_content,
            log: SubstitutionLog {
                template_name: template_name.to_string(),
                substituted: vec![],
                unsubstituted: vec![],
            },
        };
    }

    let template_content = context
        .registry()
        .get_template("commit_message_xml")
        .unwrap_or_else(|_| include_str!("templates/commit_message_xml.txt").to_string());
    let template = Template::new(&template_content);
    let partials = get_shared_partials();
    let variables = HashMap::from([
        ("DIFF", diff_content.to_string()),
        (
            "COMMIT_MESSAGE_XML_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xml"),
        ),
        (
            "COMMIT_MESSAGE_XSD_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xsd"),
        ),
    ]);

    match template.render_with_log(template_name, &variables, &partials) {
        Ok(rendered) => rendered,
        Err(e) => {
            eprintln!("Warning: Failed to render commit template: {e}");
            // Last resort: simple inline prompt with manual log
            let prompt_content = format!(
                "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
                 Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
            );
            RenderedTemplate {
                content: prompt_content,
                log: SubstitutionLog {
                    template_name: template_name.to_string(),
                    substituted: vec![SubstitutionEntry {
                        name: "DIFF".to_string(),
                        source: SubstitutionSource::Value,
                    }],
                    unsubstituted: vec![],
                },
            }
        }
    }
}

/// Generate prompt for creating commit message from provided diff using template registry.
///
/// This version uses the template registry which supports user template overrides.
/// It's the recommended way to generate prompts going forward.
///
/// # Arguments
///
/// * `context` - Template context containing the template registry
/// * `diff` - The git diff to generate a commit message for
/// * `workspace` - Workspace for resolving absolute paths (accepts any Workspace implementation)
pub fn prompt_generate_commit_message_with_diff_with_context(
    context: &TemplateContext,
    diff: &str,
    workspace: &dyn Workspace,
) -> String {
    // Ensure the commit XSD schema is available on disk for agents to reference.
    // In production this is also written during app bootstrap, but tests and some
    // entrypoints may call prompt generation directly.
    let tmp_dir = std::path::Path::new(".agent/tmp");
    let _ = workspace.create_dir_all(tmp_dir);
    let _ = workspace.write(
        &tmp_dir.join("commit_message.xsd"),
        COMMIT_MESSAGE_XSD_SCHEMA,
    );

    // Check if diff is empty or whitespace-only
    let diff_content = diff.trim();
    let has_changes = !diff_content.is_empty();

    if !has_changes {
        return "ERROR: Empty diff provided. This indicates a bug in the caller - \
                meaningful changes should be checked before requesting a commit message."
            .to_string();
    }

    let template_content = context
        .registry()
        .get_template("commit_message_xml")
        .unwrap_or_else(|_| include_str!("templates/commit_message_xml.txt").to_string());
    let template = Template::new(&template_content);
    let partials = get_shared_partials();
    let variables = HashMap::from([
        ("DIFF", diff_content.to_string()),
        (
            "COMMIT_MESSAGE_XML_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xml"),
        ),
        (
            "COMMIT_MESSAGE_XSD_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xsd"),
        ),
    ]);

    template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|e| {
        eprintln!("Warning: Failed to render commit template: {e}");
        // Last resort: simple inline prompt (no fallback template needed)
        format!(
            "Generate a conventional commit message for this diff:\n\n{diff_content}\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>"
        )
    })
}

/// Generate XSD validation retry prompt for commit message XML with substitution log.
///
/// This is the new log-based version that returns both content and substitution tracking.
/// Use this version in handlers to enable log-based validation.
pub fn prompt_commit_xsd_retry_with_log(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
    template_name: &str,
) -> RenderedTemplate {
    use std::path::Path;

    // Ensure the schema file is present.
    let tmp_dir = Path::new(".agent/tmp");
    let _ = workspace.create_dir_all(tmp_dir);
    let _ = workspace.write(
        &tmp_dir.join("commit_message.xsd"),
        COMMIT_MESSAGE_XSD_SCHEMA,
    );

    // Check that required files exist
    let schema_path = Path::new(".agent/tmp/commit_message.xsd");
    let canonical_output_path = Path::new(".agent/tmp/commit_message.xml");
    let processed_output_path = Path::new(".agent/tmp/commit_message.xml.processed");

    let schema_exists = workspace.exists(schema_path);
    let canonical_output_exists = workspace.exists(canonical_output_path);
    let processed_output_exists = workspace.exists(processed_output_path);

    // If canonical file was archived, try using the .processed file as fallback
    let (last_output_path, last_output_exists, used_processed) =
        if !canonical_output_exists && processed_output_exists {
            (processed_output_path, true, true)
        } else {
            (canonical_output_path, canonical_output_exists, false)
        };

    // Build diagnostic prefix for missing files
    let mut diagnostic_prefix = String::new();
    if !schema_exists || !last_output_exists {
        diagnostic_prefix.push_str("⚠️  WARNING: Required XSD retry files are missing:\n");
        if !schema_exists {
            writeln!(
                diagnostic_prefix,
                "  - Schema file: {} (workspace.root() = {})",
                workspace.absolute_str(".agent/tmp/commit_message.xsd"),
                workspace.root().display()
            )
            .unwrap();
        }
        if !last_output_exists {
            if used_processed {
                writeln!(
                    diagnostic_prefix,
                    "  - Last output: Neither canonical nor processed file exists:\n\
                     \t  Tried: {}\n\
                     \t  Tried: {}\n\
                     \t  (workspace.root() = {})",
                    workspace.absolute_str(".agent/tmp/commit_message.xml"),
                    workspace.absolute_str(".agent/tmp/commit_message.xml.processed"),
                    workspace.root().display()
                )
                .unwrap();
            } else {
                let processed_note = if processed_output_exists {
                    " (note: .processed file exists but canonical file is missing)"
                } else {
                    ""
                };
                writeln!(
                    diagnostic_prefix,
                    "  - Last output: {}{}\n\
                     \t  (workspace.root() = {})",
                    workspace.absolute_str(
                        canonical_output_path
                            .to_str()
                            .unwrap_or(".agent/tmp/commit_message.xml")
                    ),
                    processed_note,
                    workspace.root().display()
                )
                .unwrap();
            }
        }
        diagnostic_prefix
            .push_str("This likely indicates CWD != workspace.root() path mismatch.\n\n");
    }

    // If both files are missing, return fallback with manual log
    if !schema_exists && !last_output_exists {
        let prompt_content = format!(
            "{diagnostic_prefix}XSD VALIDATION FAILED - GENERATE COMMIT MESSAGE\n\n\
             Error: {xsd_error}\n\n\
             The schema and previous output files could not be found. \
             Please generate a conventional commit message for the current changes.\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>\n"
        );
        return RenderedTemplate {
            content: prompt_content,
            log: SubstitutionLog {
                template_name: template_name.to_string(),
                substituted: vec![SubstitutionEntry {
                    name: "XSD_ERROR".to_string(),
                    source: SubstitutionSource::Value,
                }],
                unsubstituted: vec![],
            },
        };
    }

    // Proceed with normal XSD retry prompt generation using render_with_log
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("commit_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/commit_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "COMMIT_MESSAGE_XML_PATH",
            workspace.absolute_str(
                last_output_path
                    .to_str()
                    .unwrap_or(".agent/tmp/commit_message.xml"),
            ),
        ),
        (
            "COMMIT_MESSAGE_XSD_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xsd"),
        ),
    ]);

    let template = Template::new(&template_content);
    if let Ok(mut rendered) = template.render_with_log(template_name, &variables, &partials) {
        // Prepend diagnostic prefix if files were missing but we continued anyway
        if !diagnostic_prefix.is_empty() {
            rendered.content = format!("{}\n{}", diagnostic_prefix, rendered.content);
        }
        rendered
    } else {
        // Fallback with manual log
        let prompt_content = format!(
            "XSD VALIDATION FAILED - FIX XML ONLY\n\nError: {xsd_error}\n\n\
             Read .agent/tmp/commit_message.xsd for the schema and .agent/tmp/commit_message.xml for your previous output.\n\
             Rewrite .agent/tmp/commit_message.xml with valid XML.\n"
        );
        RenderedTemplate {
            content: prompt_content,
            log: SubstitutionLog {
                template_name: template_name.to_string(),
                substituted: vec![SubstitutionEntry {
                    name: "XSD_ERROR".to_string(),
                    source: SubstitutionSource::Value,
                }],
                unsubstituted: vec![],
            },
        }
    }
}

/// Generate XSD validation retry prompt for commit message XML.
///
/// This prompt is used when a commit message XML output fails XSD validation.
///
/// The agent should read the XSD schema and the previous output from
/// `.agent/tmp/commit_message.xsd` and `.agent/tmp/commit_message.xml`, then rewrite the XML
/// to conform to the schema.
///
/// Per acceptance criteria #5: Template rendering errors must never terminate the pipeline.
/// If required files are missing, a deterministic fallback prompt is produced that includes
/// diagnostic information but still provides valid instructions to the agent.
pub fn prompt_commit_xsd_retry_with_context(
    context: &TemplateContext,
    xsd_error: &str,
    workspace: &dyn Workspace,
) -> String {
    use std::path::Path;

    // Ensure the schema file is present.
    // Note: Silent failure (let _) is acceptable here because if the schema file
    // write fails, the subsequent workspace.exists(schema_path) check will return
    // false and generate a fallback prompt with diagnostic information.
    // This approach avoids unnecessary error handling while still providing actionable feedback.
    let tmp_dir = Path::new(".agent/tmp");
    let _ = workspace.create_dir_all(tmp_dir);
    let _ = workspace.write(
        &tmp_dir.join("commit_message.xsd"),
        COMMIT_MESSAGE_XSD_SCHEMA,
    );

    // Check that required files exist
    let schema_path = Path::new(".agent/tmp/commit_message.xsd");
    let canonical_output_path = Path::new(".agent/tmp/commit_message.xml");
    let processed_output_path = Path::new(".agent/tmp/commit_message.xml.processed");

    let schema_exists = workspace.exists(schema_path);
    let canonical_output_exists = workspace.exists(canonical_output_path);
    let processed_output_exists = workspace.exists(processed_output_path);

    // If canonical file was archived, try using the .processed file as fallback
    let (last_output_path, last_output_exists, used_processed) =
        if !canonical_output_exists && processed_output_exists {
            (processed_output_path, true, true)
        } else {
            (canonical_output_path, canonical_output_exists, false)
        };

    // Build diagnostic prefix for missing files (per acceptance criteria #3)
    let mut diagnostic_prefix = String::new();
    if !schema_exists || !last_output_exists {
        diagnostic_prefix.push_str("⚠️  WARNING: Required XSD retry files are missing:\n");
        if !schema_exists {
            writeln!(
                diagnostic_prefix,
                "  - Schema file: {} (workspace.root() = {})",
                workspace.absolute_str(".agent/tmp/commit_message.xsd"),
                workspace.root().display()
            )
            .unwrap();
        }
        if !last_output_exists {
            // Show both attempted paths for clarity
            if used_processed {
                // We tried processed as fallback and it's also missing
                writeln!(
                    diagnostic_prefix,
                    "  - Last output: Neither canonical nor processed file exists:\n\
                     \t  Tried: {}\n\
                     \t  Tried: {}\n\
                     \t  (workspace.root() = {})",
                    workspace.absolute_str(".agent/tmp/commit_message.xml"),
                    workspace.absolute_str(".agent/tmp/commit_message.xml.processed"),
                    workspace.root().display()
                )
                .unwrap();
            } else {
                // Canonical path doesn't exist
                let processed_note = if processed_output_exists {
                    " (note: .processed file exists but canonical file is missing)"
                } else {
                    ""
                };
                writeln!(
                    diagnostic_prefix,
                    "  - Last output: {}{}\n\
                     \t  (workspace.root() = {})",
                    workspace.absolute_str(
                        canonical_output_path
                            .to_str()
                            .unwrap_or(".agent/tmp/commit_message.xml")
                    ),
                    processed_note,
                    workspace.root().display()
                )
                .unwrap();
            }
        }
        diagnostic_prefix
            .push_str("This likely indicates CWD != workspace.root() path mismatch.\n\n");
    }

    // If both files are missing, return fallback prompt with diagnostics (per AC #5)
    if !schema_exists && !last_output_exists {
        return format!(
            "{diagnostic_prefix}XSD VALIDATION FAILED - GENERATE COMMIT MESSAGE\n\n\
             Error: {xsd_error}\n\n\
             The schema and previous output files could not be found. \
             Please generate a conventional commit message for the current changes.\n\n\
             Output format: <ralph-commit><ralph-subject>type: description</ralph-subject></ralph-commit>\n"
        );
    }

    // Proceed with normal XSD retry prompt generation if at least schema exists
    let partials = get_shared_partials();
    let template_content = context
        .registry()
        .get_template("commit_xsd_retry")
        .unwrap_or_else(|_| include_str!("templates/commit_xsd_retry.txt").to_string());
    let variables = HashMap::from([
        ("XSD_ERROR", xsd_error.to_string()),
        (
            "COMMIT_MESSAGE_XML_PATH",
            workspace.absolute_str(
                last_output_path
                    .to_str()
                    .unwrap_or(".agent/tmp/commit_message.xml"),
            ),
        ),
        (
            "COMMIT_MESSAGE_XSD_PATH",
            workspace.absolute_str(".agent/tmp/commit_message.xsd"),
        ),
    ]);

    let template = Template::new(&template_content);
    let rendered_prompt = template
        .render_with_partials(&variables, &partials)
        .unwrap_or_else(|_| {
            format!(
                "XSD VALIDATION FAILED - FIX XML ONLY\n\nError: {xsd_error}\n\n\
                 Read .agent/tmp/commit_message.xsd for the schema and .agent/tmp/commit_message.xml for your previous output.\n\
                 Rewrite .agent/tmp/commit_message.xml with valid XML.\n"
            )
        });

    // Prepend diagnostic prefix if files were missing but we continued anyway
    if diagnostic_prefix.is_empty() {
        rendered_prompt
    } else {
        format!("{diagnostic_prefix}\n{rendered_prompt}")
    }
}

#[cfg(test)]
mod tests;
