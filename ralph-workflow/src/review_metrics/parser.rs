//! Parsing Helpers
//!
//! Helper functions for extracting issue data from ISSUES.md content.

/// Extract description from an issue line
pub(super) fn extract_description(line: &str, severity_str: &str) -> String {
    // Find where severity marker ends
    let lower = line.to_lowercase();
    if let Some(pos) = lower.find(&severity_str.to_lowercase()) {
        let after_severity = &line[pos + severity_str.len()..];
        // Skip any : or whitespace
        let desc = after_severity.trim_start_matches(':').trim();
        // Remove file:line reference if present
        if let Some(start) = desc.find('[') {
            if let Some(end) = desc.find(']') {
                if start < end {
                    let before = desc[..start].trim();
                    let after = desc[end + 1..].trim();
                    return format!("{before}{after}").trim().to_string();
                }
            }
        }
        desc.to_string()
    } else {
        line.to_string()
    }
}

/// Extract file path and line number from issue line
pub(super) fn extract_file_line(line: &str) -> (Option<String>, Option<u32>) {
    // Look for [file:line] or [file] pattern
    if let Some(start) = line.find('[') {
        if let Some(end) = line.find(']') {
            if start < end {
                let reference = &line[start + 1..end];
                if let Some(colon_pos) = reference.rfind(':') {
                    let file = reference[..colon_pos].trim();
                    let line_str = reference[colon_pos + 1..].trim();
                    if let Ok(line_num) = line_str.parse::<u32>() {
                        return (Some(file.to_string()), Some(line_num));
                    }
                }
                // Just file, no line number
                if !reference.is_empty() {
                    return (Some(reference.to_string()), None);
                }
            }
        }
    }
    (None, None)
}
