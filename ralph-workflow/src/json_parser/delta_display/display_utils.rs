// Display utilities for delta rendering.
//
// Contains constants and helper functions for sanitizing content for display.

/// ANSI escape sequence for clearing the entire line.
///
/// This is more complete than `\x1b[0K` which only clears to the end of line.
/// Using `\x1b[2K` ensures the entire line is cleared during in-place updates.
pub const CLEAR_LINE: &str = "\x1b[2K";

/// Sanitize content for single-line display during streaming.
///
/// This function prepares streamed content for in-place terminal display by:
/// - Replacing newlines with spaces (to prevent artificial line breaks)
/// - Collapsing multiple consecutive whitespace characters into single spaces
/// - Trimming leading and trailing whitespace
///
/// NOTE: This function does NOT truncate to terminal width. Truncation during
/// streaming causes visible "..." cut-offs as content accumulates. Terminal width
/// truncation should only be applied for final/non-streaming display.
///
/// # Arguments
/// * `content` - The raw content to sanitize
///
/// # Returns
/// A sanitized string suitable for single-line display, without truncation.
pub fn sanitize_for_display(content: &str) -> String {
    // Replace all whitespace (including \n, \r, \t) with spaces, then collapse multiple spaces
    let mut result = String::with_capacity(content.len());
    let mut prev_was_whitespace = false;

    for ch in content.chars() {
        if ch.is_whitespace() {
            if !prev_was_whitespace {
                result.push(' ');
                prev_was_whitespace = true;
            }
            // Skip consecutive whitespace characters
        } else {
            result.push(ch);
            prev_was_whitespace = false;
        }
    }

    // Trim leading and trailing whitespace for display
    result.trim().to_string()
}
