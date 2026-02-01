//! Development phase execution.
//!
//! This module handles the development phase of the Ralph pipeline, which consists
//! of iterative planning and execution cycles. Each iteration:
//! 1. Creates a PLAN.md from PROMPT.md
//! 2. Executes the plan
//! 3. Deletes PLAN.md
//! 4. Optionally runs fast checks

use crate::files::llm_output_extraction::PlanElements;

/// Format plan elements as markdown for PLAN.md.
pub(crate) fn format_plan_as_markdown(elements: &PlanElements) -> String {
    let mut result = String::new();

    // Summary section
    result.push_str("## Summary\n\n");
    result.push_str(&elements.summary.context);
    result.push_str("\n\n");

    // Scope items
    result.push_str("### Scope\n\n");
    for item in &elements.summary.scope_items {
        if let Some(ref count) = item.count {
            result.push_str(&format!("- **{}** {}", count, item.description));
        } else {
            result.push_str(&format!("- {}", item.description));
        }
        if let Some(ref category) = item.category {
            result.push_str(&format!(" ({})", category));
        }
        result.push('\n');
    }
    result.push('\n');

    // Implementation steps
    result.push_str("## Implementation Steps\n\n");
    for step in &elements.steps {
        // Step header
        let step_type_str = match step.step_type {
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::FileChange => {
                "file-change"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::Action => "action",
            crate::files::llm_output_extraction::xsd_validation_plan::StepType::Research => {
                "research"
            }
        };
        let priority_str = step.priority.map_or(String::new(), |p| {
            format!(
                " [{}]",
                match p {
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Critical =>
                        "critical",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::High =>
                        "high",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Medium =>
                        "medium",
                    crate::files::llm_output_extraction::xsd_validation_plan::Priority::Low =>
                        "low",
                }
            )
        });

        result.push_str(&format!(
            "### Step {} ({}){}:  {}\n\n",
            step.number, step_type_str, priority_str, step.title
        ));

        // Target files
        if !step.target_files.is_empty() {
            result.push_str("**Target Files:**\n");
            for tf in &step.target_files {
                let action_str = match tf.action {
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Create => {
                        "create"
                    }
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Modify => {
                        "modify"
                    }
                    crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Delete => {
                        "delete"
                    }
                };
                result.push_str(&format!("- `{}` ({})\n", tf.path, action_str));
            }
            result.push('\n');
        }

        // Location
        if let Some(ref location) = step.location {
            result.push_str(&format!("**Location:** {}\n\n", location));
        }

        // Rationale
        if let Some(ref rationale) = step.rationale {
            result.push_str(&format!("**Rationale:** {}\n\n", rationale));
        }

        // Content
        result.push_str(&format_rich_content(&step.content));
        result.push('\n');

        // Dependencies
        if !step.depends_on.is_empty() {
            result.push_str("**Depends on:** ");
            let deps: Vec<String> = step
                .depends_on
                .iter()
                .map(|d| format!("Step {}", d))
                .collect();
            result.push_str(&deps.join(", "));
            result.push_str("\n\n");
        }
    }

    // Critical files
    result.push_str("## Critical Files\n\n");
    result.push_str("### Primary Files\n\n");
    for pf in &elements.critical_files.primary_files {
        let action_str = match pf.action {
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Create => {
                "create"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Modify => {
                "modify"
            }
            crate::files::llm_output_extraction::xsd_validation_plan::FileAction::Delete => {
                "delete"
            }
        };
        if let Some(ref est) = pf.estimated_changes {
            result.push_str(&format!("- `{}` ({}) - {}\n", pf.path, action_str, est));
        } else {
            result.push_str(&format!("- `{}` ({})\n", pf.path, action_str));
        }
    }
    result.push('\n');

    if !elements.critical_files.reference_files.is_empty() {
        result.push_str("### Reference Files\n\n");
        for rf in &elements.critical_files.reference_files {
            result.push_str(&format!("- `{}` - {}\n", rf.path, rf.purpose));
        }
        result.push('\n');
    }

    // Risks and mitigations
    result.push_str("## Risks & Mitigations\n\n");
    for rp in &elements.risks_mitigations {
        let severity_str = rp.severity.map_or(String::new(), |s| {
            format!(
                " [{}]",
                match s {
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Low =>
                        "low",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Medium =>
                        "medium",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::High =>
                        "high",
                    crate::files::llm_output_extraction::xsd_validation_plan::Severity::Critical =>
                        "critical",
                }
            )
        });
        result.push_str(&format!("**Risk{}:** {}\n", severity_str, rp.risk));
        result.push_str(&format!("**Mitigation:** {}\n\n", rp.mitigation));
    }

    // Verification strategy
    result.push_str("## Verification Strategy\n\n");
    for (i, v) in elements.verification_strategy.iter().enumerate() {
        result.push_str(&format!("{}. **{}**\n", i + 1, v.method));
        result.push_str(&format!("   Expected: {}\n\n", v.expected_outcome));
    }

    result
}

/// Format rich content elements to markdown.
fn format_rich_content(
    content: &crate::files::llm_output_extraction::xsd_validation_plan::RichContent,
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::ContentElement;

    let mut result = String::new();

    for element in &content.elements {
        match element {
            ContentElement::Paragraph(p) => {
                result.push_str(&format_inline_content(&p.content));
                result.push_str("\n\n");
            }
            ContentElement::CodeBlock(cb) => {
                let lang = cb.language.as_deref().unwrap_or("");
                result.push_str(&format!("```{}\n", lang));
                result.push_str(&cb.content);
                if !cb.content.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("```\n\n");
            }
            ContentElement::Table(t) => {
                if let Some(ref caption) = t.caption {
                    result.push_str(&format!("**{}**\n\n", caption));
                }
                // Header row
                if !t.columns.is_empty() {
                    result.push_str("| ");
                    result.push_str(&t.columns.join(" | "));
                    result.push_str(" |\n");
                    result.push('|');
                    for _ in &t.columns {
                        result.push_str(" --- |");
                    }
                    result.push('\n');
                } else if let Some(first_row) = t.rows.first() {
                    // Infer column count from first row
                    result.push('|');
                    for _ in &first_row.cells {
                        result.push_str(" --- |");
                    }
                    result.push('\n');
                }
                // Data rows
                for row in &t.rows {
                    result.push_str("| ");
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| format_inline_content(&c.content))
                        .collect();
                    result.push_str(&cells.join(" | "));
                    result.push_str(" |\n");
                }
                result.push('\n');
            }
            ContentElement::List(l) => {
                result.push_str(&format_list(l, 0));
                result.push('\n');
            }
            ContentElement::Heading(h) => {
                let prefix = "#".repeat(h.level as usize);
                result.push_str(&format!("{} {}\n\n", prefix, h.text));
            }
        }
    }

    result
}

/// Format inline content elements.
fn format_inline_content(
    content: &[crate::files::llm_output_extraction::xsd_validation_plan::InlineElement],
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::InlineElement;

    content
        .iter()
        .map(|e| match e {
            InlineElement::Text(s) => s.clone(),
            InlineElement::Emphasis(s) => format!("**{}**", s),
            InlineElement::Code(s) => format!("`{}`", s),
            InlineElement::Link { href, text } => format!("[{}]({})", text, href),
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Format a list element with proper indentation.
fn format_list(
    list: &crate::files::llm_output_extraction::xsd_validation_plan::List,
    indent: usize,
) -> String {
    use crate::files::llm_output_extraction::xsd_validation_plan::ListType;

    let mut result = String::new();
    let indent_str = "  ".repeat(indent);

    for (i, item) in list.items.iter().enumerate() {
        let marker = match list.list_type {
            ListType::Ordered => format!("{}. ", i + 1),
            ListType::Unordered => "- ".to_string(),
        };

        result.push_str(&indent_str);
        result.push_str(&marker);
        result.push_str(&format_inline_content(&item.content));
        result.push('\n');

        if let Some(ref nested) = item.nested_list {
            result.push_str(&format_list(nested, indent + 1));
        }
    }

    result
}
