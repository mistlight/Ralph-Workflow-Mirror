//! Text-based extraction for fallback when JSON result events are not available.
//!
//! Note: Currently unused in production (XML extraction is used instead).
//! Kept for potential future use and test compatibility.

#[cfg(any(test, feature = "test-utils"))]
use super::scoring::score_text_plan;

/// Extract plan content from text by looking for markdown structure.
///
/// This is a fallback method for cases where JSON result events are not available.
/// It looks for common plan markers like `## Summary` and `## Implementation Steps`.
/// If multiple plan candidates are found, it returns the highest-scoring one.
/// If no markers are found, it falls back to extracting substantial text content
/// that contains plan-like keywords.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn extract_plan_from_text(content: &str) -> Option<String> {
    // Look for plan start markers - these indicate where a plan begins
    let start_markers = [
        "## Summary",
        "# Plan",
        "# Implementation Plan",
        "## Implementation Steps",
    ];

    // Find all potential plan candidates
    // Each candidate starts at a marker and continues to the end of content
    let mut candidates: Vec<(usize, &str)> = Vec::new();

    for marker in start_markers {
        if let Some(start) = content.find(marker) {
            // Extract from the marker to the end of the content
            let plan_content = &content[start..];
            let trimmed = plan_content.trim();

            if trimmed.len() > 50 {
                candidates.push((start, trimmed));
            }
        }
    }

    if !candidates.is_empty() {
        // Score each candidate and return the best one
        let mut best_candidate: Option<&str> = None;
        let mut best_score: u32 = 0;

        for (_start, candidate) in &candidates {
            let score = score_text_plan(candidate);
            if score > best_score {
                best_score = score;
                best_candidate = Some(candidate);
            }
        }

        if candidates.len() > 1 {
            eprintln!(
                "[result_extraction] Found {} plan candidates in text, selected one with score {}",
                candidates.len(),
                best_score
            );
        }

        return best_candidate.map(std::string::ToString::to_string);
    }

    // Permissive fallback: if no markdown markers found, look for substantial
    // content that contains plan-like keywords. This handles plaintext mode where
    // the agent outputs plan content without structured markdown.
    extract_plan_from_text_permissive(content)
}

/// Permissive extraction that finds substantial plan-like content without
/// requiring specific markdown markers.
///
/// This is a final fallback for plaintext mode logs where the agent may have
/// output a valid plan but without the expected markdown structure.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn extract_plan_from_text_permissive(content: &str) -> Option<String> {
    // Minimum content length (increased from 50 to 200 for permissive mode)
    const MIN_PERMISSIVE_LENGTH: usize = 200;

    let content = content.trim();

    // Filter out obvious non-plan content
    // - JSON lines
    // - Debug/tool output patterns
    let filtered: String = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip JSON lines
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                return false;
            }
            // Skip debug/tool markers
            if trimmed.starts_with("[debug]")
                || trimmed.starts_with("[tool]")
                || trimmed.starts_with("[error]")
                || trimmed.starts_with("[warn]")
            {
                return false;
            }
            // Skip empty lines
            if trimmed.is_empty() {
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n");

    if filtered.len() < MIN_PERMISSIVE_LENGTH {
        return None;
    }

    // Check for plan-like keywords (case-insensitive)
    let plan_keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "develop",
        "write",
        "function",
        "feature",
        "component",
        "module",
        "task",
        "phase",
        "first",
        "second",
        "third",
        "next",
        "then",
        "finally",
        "approach",
        "strategy",
        "design",
        "architecture",
    ];

    let filtered_lower = filtered.to_lowercase();
    let has_plan_keyword = plan_keywords
        .iter()
        .any(|keyword| filtered_lower.contains(keyword));

    if has_plan_keyword {
        return Some(filtered);
    }

    None
}
