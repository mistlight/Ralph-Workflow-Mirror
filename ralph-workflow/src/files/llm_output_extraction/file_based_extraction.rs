//! File-based XML extraction utilities.
//!
//! This module provides functions for extracting XML from designated file locations.
//! Agents write XML to `.agent/tmp/` files for faster iteration cycles.
//!
//! # Benefits
//!
//! - **Faster retries**: Agents can edit existing files instead of regenerating entire XML
//! - **Self-validation**: Agents can use `xmllint` to catch errors before submitting
//! - **Cleaner separation**: XML lives in files, not embedded in text responses
//! - **Better debugging**: XML files persist for inspection

use std::fs;
use std::path::Path;

/// XML file paths by phase.
pub mod paths {
    /// Path for planning phase XML output
    pub const PLAN_XML: &str = ".agent/tmp/plan.xml";
    /// Path for development result XML output
    pub const DEVELOPMENT_RESULT_XML: &str = ".agent/tmp/development_result.xml";
    /// Path for review issues XML output
    pub const ISSUES_XML: &str = ".agent/tmp/issues.xml";
    /// Path for fix result XML output
    pub const FIX_RESULT_XML: &str = ".agent/tmp/fix_result.xml";
    /// Path for commit message XML output
    pub const COMMIT_MESSAGE_XML: &str = ".agent/tmp/commit_message.xml";
}

/// Resolve a relative `.agent/tmp/` path to an absolute path.
///
/// This function is critical for AI agents with security sandboxes that reject
/// relative paths as "outside the working directory". By providing absolute paths,
/// we ensure agents can write to `.agent/tmp/` without security violations.
///
/// # Arguments
///
/// * `relative_path` - A relative path like `.agent/tmp/issues.xml`
///
/// # Returns
///
/// An absolute path if working directory can be determined, otherwise the original
/// relative path as a fallback.
///
/// # Example
///
/// ```ignore
/// // Working dir: /Users/user/project
/// let path = resolve_absolute_path(".agent/tmp/issues.xml");
/// // Returns: "/Users/user/project/.agent/tmp/issues.xml"
/// ```
///
/// **Note:** This function uses the current working directory for paths.
/// For explicit path control, use [`resolve_absolute_path_at`] instead.
pub fn resolve_absolute_path(relative_path: &str) -> String {
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(relative_path).display().to_string())
        .unwrap_or_else(|| relative_path.to_string())
}

/// Resolve a relative path to an absolute path at a specific repository root.
///
/// This is used to provide agents with absolute paths in prompts, ensuring
/// they write files to the correct locations.
///
/// # Arguments
///
/// * `repo_root` - Path to the repository root
/// * `relative_path` - The relative path to resolve (e.g., ".agent/tmp/issues.xml")
///
/// # Returns
///
/// The absolute path as a string.
///
/// # Example
///
/// ```ignore
/// // repo_root: /Users/user/project
/// let path = resolve_absolute_path_at(Path::new("/Users/user/project"), ".agent/tmp/issues.xml");
/// // Returns: "/Users/user/project/.agent/tmp/issues.xml"
/// ```
pub fn resolve_absolute_path_at(repo_root: &std::path::Path, relative_path: &str) -> String {
    repo_root.join(relative_path).display().to_string()
}

/// Try to read XML from a designated file location.
///
/// Returns `Some(xml_content)` if the file exists and contains non-empty XML-like content.
/// Returns `None` if the file doesn't exist, is empty, or doesn't look like XML.
///
/// # Arguments
///
/// * `xml_path` - Path to the XML file
///
/// # Example
///
/// ```ignore
/// if let Some(xml) = try_extract_from_file(Path::new(".agent/tmp/plan.xml")) {
///     // Use the extracted XML
/// }
/// ```
pub fn try_extract_from_file(xml_path: &Path) -> Option<String> {
    if !xml_path.exists() {
        return None;
    }

    let content = fs::read_to_string(xml_path).ok()?;
    let trimmed = content.trim();

    // Must be non-empty and look like XML
    if trimmed.is_empty() || !trimmed.starts_with('<') {
        return None;
    }

    Some(trimmed.to_string())
}

/// Combined extraction: try file first, then fall back to response extraction.
///
/// This function implements the two-tier extraction strategy:
/// 1. First, check if XML was written to the designated file
/// 2. If not found, fall back to extracting XML from the response content
///
/// # Arguments
///
/// * `xml_path` - Path to check for file-based XML
/// * `response_content` - The response content to fall back to
/// * `extractor` - Function to extract XML from response content
///
/// # Example
///
/// ```ignore
/// let xml = extract_xml_with_file_fallback(
///     Path::new(".agent/tmp/plan.xml"),
///     response_content,
///     |content| extract_plan_xml(content),
/// );
/// ```
pub fn extract_xml_with_file_fallback<F>(
    xml_path: &Path,
    response_content: &str,
    extractor: F,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    // Try file-based extraction first
    if let Some(xml) = try_extract_from_file(xml_path) {
        return Some(xml);
    }

    // Fall back to response extraction
    extractor(response_content)
}

/// Archive an XML output file after successful processing.
///
/// Moves the XML file to a `.processed` suffix so it's preserved for debugging
/// but clearly marked as already processed. If a `.processed` file already exists,
/// it is overwritten.
///
/// # Arguments
///
/// * `xml_path` - Path to the XML file to archive
pub fn archive_xml_file(xml_path: &Path) {
    if xml_path.exists() {
        let processed_path = xml_path.with_extension("xml.processed");
        let _ = fs::rename(xml_path, processed_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_try_extract_from_file_success() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("test.xml");
        fs::write(
            &xml_path,
            "<ralph-plan><ralph-summary>Test</ralph-summary></ralph-plan>",
        )
        .unwrap();

        let result = try_extract_from_file(&xml_path);
        assert!(result.is_some());
        assert!(result.unwrap().contains("<ralph-plan>"));
    }

    #[test]
    fn test_try_extract_from_file_not_exists() {
        let result = try_extract_from_file(Path::new("/nonexistent/file.xml"));
        assert!(result.is_none());
    }

    #[test]
    fn test_try_extract_from_file_empty() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("empty.xml");
        fs::write(&xml_path, "").unwrap();

        let result = try_extract_from_file(&xml_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_extract_from_file_whitespace_only() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("whitespace.xml");
        fs::write(&xml_path, "   \n  \n  ").unwrap();

        let result = try_extract_from_file(&xml_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_extract_from_file_not_xml() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("not_xml.txt");
        fs::write(&xml_path, "This is plain text, not XML").unwrap();

        let result = try_extract_from_file(&xml_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_extract_from_file_trims_whitespace() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("padded.xml");
        fs::write(&xml_path, "  \n<ralph-plan>Test</ralph-plan>\n  ").unwrap();

        let result = try_extract_from_file(&xml_path);
        assert!(result.is_some());
        let xml = result.unwrap();
        assert!(xml.starts_with('<'));
        assert!(xml.ends_with('>'));
    }

    #[test]
    fn test_extract_xml_with_file_fallback_prefers_file() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("file.xml");
        fs::write(&xml_path, "<from-file>content</from-file>").unwrap();

        let response = "<from-response>other</from-response>";
        let result = extract_xml_with_file_fallback(&xml_path, response, |_| {
            Some("<from-extractor>fallback</from-extractor>".to_string())
        });

        assert!(result.is_some());
        assert!(result.unwrap().contains("<from-file>"));
    }

    #[test]
    fn test_extract_xml_with_file_fallback_uses_extractor() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("missing.xml");

        let response = "response content";
        let result = extract_xml_with_file_fallback(&xml_path, response, |content| {
            if content == "response content" {
                Some("<extracted>from response</extracted>".to_string())
            } else {
                None
            }
        });

        assert!(result.is_some());
        assert!(result.unwrap().contains("<extracted>"));
    }

    #[test]
    fn test_extract_xml_with_file_fallback_returns_none() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("missing.xml");

        let result = extract_xml_with_file_fallback(&xml_path, "no xml here", |_| None);

        assert!(result.is_none());
    }

    #[test]
    fn test_archive_xml_file_moves_file() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("to_archive.xml");
        let processed_path = dir.path().join("to_archive.xml.processed");
        fs::write(&xml_path, "<test>content</test>").unwrap();
        assert!(xml_path.exists());

        archive_xml_file(&xml_path);
        assert!(!xml_path.exists());
        assert!(processed_path.exists());
        assert_eq!(
            fs::read_to_string(&processed_path).unwrap(),
            "<test>content</test>"
        );
    }

    #[test]
    fn test_archive_xml_file_handles_missing() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("nonexistent.xml");
        // Should not panic
        archive_xml_file(&xml_path);
    }

    #[test]
    fn test_archive_xml_file_overwrites_existing_processed() {
        let dir = TempDir::new().unwrap();
        let xml_path = dir.path().join("test.xml");
        let processed_path = dir.path().join("test.xml.processed");

        // Create old processed file
        fs::write(&processed_path, "<old>data</old>").unwrap();
        // Create new XML file
        fs::write(&xml_path, "<new>data</new>").unwrap();

        archive_xml_file(&xml_path);
        assert!(!xml_path.exists());
        assert!(processed_path.exists());
        assert_eq!(
            fs::read_to_string(&processed_path).unwrap(),
            "<new>data</new>"
        );
    }

    #[test]
    fn test_paths_constants() {
        assert_eq!(paths::PLAN_XML, ".agent/tmp/plan.xml");
        assert_eq!(paths::ISSUES_XML, ".agent/tmp/issues.xml");
        assert_eq!(
            paths::DEVELOPMENT_RESULT_XML,
            ".agent/tmp/development_result.xml"
        );
        assert_eq!(paths::FIX_RESULT_XML, ".agent/tmp/fix_result.xml");
        assert_eq!(paths::COMMIT_MESSAGE_XML, ".agent/tmp/commit_message.xml");
    }

    #[test]
    fn test_resolve_absolute_path_with_cwd() {
        // When current_dir() succeeds, should return absolute path
        let result = resolve_absolute_path(".agent/tmp/issues.xml");

        // Should contain .agent/tmp/issues.xml at the end
        assert!(result.contains(".agent/tmp/issues.xml"));

        // Should be longer than the relative path (has directory prefix)
        assert!(result.len() > ".agent/tmp/issues.xml".len());

        // Should not start with a dot if we got the absolute path
        if std::env::current_dir().is_ok() {
            #[cfg(unix)]
            assert!(result.starts_with('/'));

            #[cfg(windows)]
            assert!(result.contains(":\\") || result.starts_with("\\\\"));
        }
    }

    #[test]
    fn test_resolve_absolute_path_fallback() {
        // Even if cwd fails, should return the input path as fallback
        let input = ".agent/tmp/test.xml";
        let result = resolve_absolute_path(input);

        // At minimum, should contain the input path
        assert!(result.contains("test.xml"));
    }

    #[test]
    fn test_resolve_absolute_path_xsd() {
        // Test with XSD file paths
        let result = resolve_absolute_path(".agent/tmp/issues.xsd");
        assert!(result.contains(".agent/tmp/issues.xsd"));
    }
}
