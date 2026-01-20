//! Scoring utilities for content quality assessment.
//!
//! Note: Currently unused in production (XML extraction is used instead).
//! Kept for potential future use and test compatibility.

/// Calculate a score for a result to determine its quality.
///
/// Higher scores indicate better results. Scoring considers:
/// - Presence of plan structure markers (## Summary, ## Implementation Steps, etc.)
/// - Markdown headers (#)
/// - Content length (longer is generally better)
/// - Plan-like keywords
#[allow(dead_code)]
pub fn score_result(content: &str) -> u32 {
    let mut score: u32 = 0;
    let content_lower = content.to_lowercase();

    // Strong structure markers (very high weight)
    let structure_markers = [
        "## Summary",
        "## Implementation Steps",
        "## Implementation",
        "### Implementation",
        "# Summary",
        "# Implementation Plan",
        "# Plan",
    ];
    for marker in &structure_markers {
        if content.contains(marker) {
            score += 1000;
        }
    }

    // Secondary headers (medium weight)
    let secondary_headers = ["###", "####", "## Risks", "## Verification", "## Testing"];
    for header in &secondary_headers {
        if content.contains(header) {
            score += 100;
        }
    }

    // Any markdown headers (low weight)
    for line in content.lines() {
        if line.trim().starts_with('#') {
            score += 10;
        }
    }

    // Plan keywords (very low weight as tiebreaker)
    let keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "task",
        "phase",
        "first",
        "second",
        "then",
        "finally",
        "next",
    ];
    for keyword in &keywords {
        if content_lower.contains(keyword) {
            score += 1;
        }
    }

    // Length bonus (slight preference for longer content with same structure)
    // Cap the bonus to avoid length overriding structure
    let length_bonus = u32::try_from(content.len()).unwrap_or(u32::MAX).min(500);
    score += length_bonus;

    score
}

/// Calculate a score for text content to determine plan completeness.
///
/// This is similar to `score_result()` but works on raw text content rather than
/// JSON result events. Higher scores indicate more complete plans.
#[allow(dead_code)]
pub fn score_text_plan(content: &str) -> u32 {
    let mut score: u32 = 0;
    let content_lower = content.to_lowercase();

    // Strong structure markers (very high weight)
    let structure_markers = [
        "## Summary",
        "## Implementation Steps",
        "## Implementation",
        "### Implementation",
        "# Summary",
        "# Implementation Plan",
        "# Plan",
    ];
    for marker in &structure_markers {
        if content.contains(marker) {
            score += 1000;
        }
    }

    // Secondary headers (medium weight)
    let secondary_headers = ["###", "####", "## Risks", "## Verification", "## Testing"];
    for header in &secondary_headers {
        if content.contains(header) {
            score += 100;
        }
    }

    // Any markdown headers (low weight)
    for line in content.lines() {
        if line.trim().starts_with('#') {
            score += 10;
        }
    }

    // Plan keywords (very low weight as tiebreaker)
    let keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "task",
        "phase",
        "first",
        "second",
        "then",
        "finally",
        "next",
    ];
    for keyword in &keywords {
        if content_lower.contains(keyword) {
            score += 1;
        }
    }

    // Length bonus (slight preference for longer content with same structure)
    // Cap the bonus to avoid length overriding structure
    let length_bonus = u32::try_from(content.len()).unwrap_or(u32::MAX).min(500);
    score += length_bonus;

    score
}
