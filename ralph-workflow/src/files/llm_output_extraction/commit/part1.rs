// ============================================================================
// Commit message extraction core types and functions
// ============================================================================

/// Result of commit message extraction.
///
/// This struct wraps a successfully extracted commit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitExtractionResult(String);

impl CommitExtractionResult {
    /// Create a new extraction result with the given message.
    pub fn new(message: String) -> Self {
        Self(message)
    }

    /// Convert into the inner message string with final escape sequence cleanup.
    ///
    /// This applies the final rendering step to ensure no escape sequences leak through
    /// to the actual commit message.
    pub fn into_message(self) -> String {
        render_final_commit_message(&self.0)
    }
}

/// Try to extract commit message from XML format with detailed tracing.
///
/// This function uses flexible XML extraction to handle various AI embedding patterns:
/// - Direct XML tags at content start
/// - XML in markdown code fences (```xml, ```)
/// - XML in JSON strings (escaped)
/// - XML embedded within analysis text
///
/// The XML format is preferred because:
/// - No escape sequence issues (actual newlines work fine)
/// - Distinctive tags unlikely to appear in LLM analysis text
/// - Clear boundaries for parsing
///
/// # Expected Format
///
/// ```xml
/// <ralph-commit>
/// <ralph-subject>type(scope): description</ralph-subject>
/// <ralph-body>Optional body text here.
/// Can span multiple lines.</ralph-body>
/// </ralph-commit>
/// ```
///
/// Or with detailed body elements:
///
/// ```xml
/// <ralph-commit>
/// <ralph-subject>type(scope): description</ralph-subject>
/// <ralph-body-summary>Brief summary</ralph-body-summary>
/// <ralph-body-details>Detailed bullet points</ralph-body-details>
/// <ralph-body-footer>BREAKING CHANGE or Fixes #123</ralph-body-footer>
/// </ralph-commit>
/// ```
///
/// The `<ralph-body>` tag is optional and may be omitted for commits without a body.
///
/// # Returns
///
/// A tuple of `(Option<String>, String)`:
/// - First element: `Some(message)` if valid XML with a valid conventional commit subject was found, `None` otherwise
/// - Second element: Detailed reason string explaining what was found/not found (for debugging)
pub fn try_extract_xml_commit_with_trace(content: &str) -> (Option<String>, String) {
    // Try flexible XML extraction that handles various AI embedding patterns.
    // If extraction fails, use the raw content directly - XSD validation will
    // provide a clear error message explaining what's wrong (e.g., missing
    // <ralph-commit> root element) that can be sent back to the AI for retry.
    let (xml_block, extraction_pattern) = match extract_xml_commit(content) {
        Some(xml) => {
            // Detect which extraction pattern was used for logging
            let pattern = if content.trim().starts_with("<ralph-commit>") {
                "direct XML"
            } else if content.contains("```xml") || content.contains("```\n<ralph-commit>") {
                "markdown code fence"
            } else if content.contains("{\"result\":") || content.contains("\"result\":") {
                "JSON string"
            } else {
                "embedded search"
            };
            (xml, pattern)
        }
        None => {
            // No XML tags found - use raw content and let XSD validation
            // produce an informative error for the AI to retry
            (content.to_string(), "raw content (no XML tags found)")
        }
    };

    // Run XSD validation - this will catch both malformed XML and missing elements
    let xsd_result = validate_xml_against_xsd(&xml_block);

    let message = match xsd_result {
        Ok(elements) => {
            // Format the commit message using parsed elements
            let body = elements.format_body();
            if body.is_empty() {
                elements.subject.clone()
            } else {
                format!("{}\n\n{}", elements.subject, body)
            }
        }
        Err(e) => {
            // XSD validation failed - return error with details for AI retry
            let error_msg = e.format_for_ai_retry();
            return (None, format!("XSD validation failed: {}", error_msg));
        }
    };

    // Determine body presence for logging
    let has_body = message.lines().count() > 1;

    // Use character-based truncation for UTF-8 safety
    let message_preview = {
        let escaped = message.replace('\n', "\\n");
        truncate_text(&escaped, 83) // ~80 chars + "..."
    };

    (
        Some(message.clone()),
        format!(
            "Found <ralph-commit> via {}, XSD validation passed, body={}, message: '{}'",
            extraction_pattern,
            if has_body { "present" } else { "absent" },
            message_preview
        ),
    )
}

/// Check if a string is a valid conventional commit subject line.
pub fn is_conventional_commit_subject(subject: &str) -> bool {
    let valid_types = [
        "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
    ];

    // Find the colon
    let Some(colon_pos) = subject.find(':') else {
        return false;
    };

    let prefix = &subject[..colon_pos];

    // Extract type (before optional scope and !)
    let type_end = prefix
        .find('(')
        .unwrap_or_else(|| prefix.find('!').unwrap_or(prefix.len()));
    let commit_type = &prefix[..type_end];

    valid_types.contains(&commit_type)
}

// =========================================================================
// Final Commit Message Rendering
// =========================================================================

/// Render the final commit message with all cleanup applied.
///
/// This is the final step before returning a commit message for use in git commit.
/// It applies:
/// 1. Escape sequence cleanup (aggressive unescaping)
/// 2. Final whitespace cleanup
///
/// # Arguments
///
/// * `message` - The commit message to render
///
/// # Returns
///
/// The fully rendered commit message with all escape sequences properly handled.
pub fn render_final_commit_message(message: &str) -> String {
    let mut result = message.to_string();

    // Step 1: Apply final escape sequence cleanup
    // This handles any escape sequences that leaked through the pipeline
    result = final_escape_sequence_cleanup(&result);

    // Step 2: Try aggressive unescaping if there are still escape sequences
    if result.contains("\\n") || result.contains("\\t") || result.contains("\\r") {
        result = unescape_json_strings_aggressive(&result);
    }

    // Step 3: Final whitespace cleanup
    result = result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    result
}
