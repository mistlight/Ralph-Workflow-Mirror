//! Commit message XML renderer.
//!
//! Renders commit message XML with semantic formatting:
//! - Box-drawing header
//! - Subject line prominently
//! - Body text with proper indentation

use super::helpers::extract_tag_content;

/// Render commit message XML with semantic formatting.
pub fn render(content: &str) -> String {
    let mut output = String::new();

    output.push_str("\n╔═══ Commit Message ═══╗\n\n");

    // Extract subject and body from commit XML
    // Note: Commit XML uses ralph-subject and ralph-body tags
    let subject = extract_tag_content(content, "ralph-subject")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let body = extract_tag_content(content, "ralph-body")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if subject.is_none() && body.is_none() {
        output.push_str("⚠️  Unable to parse commit message XML\n\n");
        output.push_str(content);
        return output;
    }

    if let Some(subject) = subject {
        output.push_str(&format!("📝 {}\n", subject));
    }

    if let Some(body) = body {
        output.push('\n');
        for line in wrap_commit_body(&body, 80).lines() {
            output.push_str(&format!("   {}\n", line));
        }
    }

    output
}

fn wrap_commit_body(body: &str, max_width: usize) -> String {
    let indent = 3usize;
    let wrap_width = max_width.saturating_sub(indent);

    body.lines()
        .map(|line| {
            let line = line.trim_end();
            if line.is_empty() {
                return String::new();
            }
            let trimmed = line.trim_start();
            let is_listish = trimmed.starts_with('-')
                || trimmed.starts_with('*')
                || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit());
            if is_listish || trimmed.len() <= wrap_width {
                return trimmed.to_string();
            }

            let mut out_lines: Vec<String> = Vec::new();
            let mut current = String::new();
            for word in trimmed.split_whitespace() {
                if current.is_empty() {
                    current.push_str(word);
                    continue;
                }
                if current.len() + 1 + word.len() > wrap_width {
                    out_lines.push(current);
                    current = word.to_string();
                } else {
                    current.push(' ');
                    current.push_str(word);
                }
            }
            if !current.is_empty() {
                out_lines.push(current);
            }
            out_lines.join("\n")
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_commit_with_subject_and_body() {
        let xml = r#"<ralph-commit>
<ralph-subject>feat: add new authentication system</ralph-subject>
<ralph-body>This commit introduces a new JWT-based authentication system.

- Added auth middleware
- Created user session management
- Updated API endpoints</ralph-body>
</ralph-commit>"#;

        let output = render(xml);

        assert!(
            output.contains("Commit Message"),
            "Should have commit header"
        );
        assert!(
            output.contains("feat: add new authentication"),
            "Should show subject"
        );
        assert!(
            output.contains("JWT-based authentication"),
            "Should show body"
        );
        assert!(
            output.contains("Added auth middleware"),
            "Should show body details"
        );
    }

    #[test]
    fn test_render_commit_subject_only() {
        let xml = r#"<ralph-commit>
<ralph-subject>fix: resolve null pointer exception</ralph-subject>
</ralph-commit>"#;

        let output = render(xml);

        assert!(
            output.contains("fix: resolve null pointer"),
            "Should show subject"
        );
    }

    #[test]
    fn test_render_commit_falls_back_to_raw_with_warning_when_subject_is_blank() {
        let xml = r#"<ralph-commit>
<ralph-subject>   </ralph-subject>
</ralph-commit>"#;

        let output = render(xml);

        assert!(output.contains("⚠️"), "Should warn on parse failure");
        assert!(
            output.contains("<ralph-commit>"),
            "Should include raw XML fallback"
        );
        assert!(
            !output.contains("📝 \n"),
            "Should not render an empty subject line"
        );
    }

    #[test]
    fn test_all_renderers_have_header_boxes() {
        // Verify commit message has box-drawing characters
        let commit_output = render("<ralph-commit>invalid</ralph-commit>");
        assert!(commit_output.contains("═"), "Commit should have box header");
    }
}
