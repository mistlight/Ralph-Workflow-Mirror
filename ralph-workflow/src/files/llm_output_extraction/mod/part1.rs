// LLM Output Extraction - Part 1: Test-only extraction function
//
// This file contains the test-only extract_llm_output function.

/// Extract result content from LLM CLI output.
///
/// This function attempts to extract meaningful content from the output of various
/// LLM CLI tools. It will:
///
/// 1. Try the specified format's extraction strategy
/// 2. Fall back to auto-detection if the specified format fails
/// 3. Fall back to plain text extraction as a last resort
///
/// # Arguments
///
/// * `output` - The raw output from the LLM CLI
/// * `format` - Optional format hint (if None, will auto-detect)
///
/// # Returns
///
/// An `ExtractionOutput` containing the extracted content and metadata.
///
/// # Example
///
/// ```ignore
/// let output = r#"{"type":"result","result":"feat: add feature"}"#;
/// let result = extract_llm_output(output, Some(OutputFormat::Claude));
/// assert_eq!(result.content, "feat: add feature");
/// ```
#[cfg(test)]
fn extract_llm_output(output: &str, format: Option<OutputFormat>) -> ExtractionOutput {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return ExtractionOutput::empty();
    }

    // Determine format - use provided or auto-detect
    let detected_format = format.unwrap_or_else(|| detect_output_format(trimmed));

    // Try the detected format first
    if let Some(content) = extract_by_format(trimmed, detected_format) {
        return ExtractionOutput::structured(content, detected_format);
    }

    // If specified format failed, try auto-detection with all formats
    if format.is_some() {
        for try_format in [
            OutputFormat::Claude,
            OutputFormat::Codex,
            OutputFormat::Gemini,
            OutputFormat::OpenCode,
        ] {
            if try_format != detected_format {
                if let Some(content) = extract_by_format(trimmed, try_format) {
                    return ExtractionOutput::structured(content, try_format);
                }
            }
        }
    }

    // Fall back to plain text extraction
    let cleaned = clean_plain_text(trimmed);
    if cleaned.is_empty() {
        ExtractionOutput::empty()
    } else {
        ExtractionOutput::fallback(
            cleaned,
            "Used plain text fallback - no structured format detected",
        )
    }
}
