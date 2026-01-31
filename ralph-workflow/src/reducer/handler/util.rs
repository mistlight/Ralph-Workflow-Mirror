use crate::files::llm_output_extraction::archive_xml_file_with_workspace;
use crate::files::llm_output_extraction::file_based_extraction::paths as xml_paths;
use crate::reducer::ui_event::XmlCodeSnippet;
use crate::workspace::Workspace;
use regex::Regex;
use std::path::Path;

pub(super) fn read_commit_message_xml(workspace: &dyn Workspace) -> Option<String> {
    read_xml_and_archive_if_present(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML))
}

/// Read XML content from the primary path only.
///
/// The reducer/effect pipeline requires agents to write XML to the canonical
/// `.agent/tmp/*.xml` paths. Archived `.processed` files are debug artifacts and
/// must not be used as fallback inputs.
pub(super) fn read_xml_if_present(
    workspace: &dyn Workspace,
    primary_path: &Path,
) -> Option<String> {
    workspace.read(primary_path).ok()
}

pub(super) fn read_xml_and_archive_if_present(
    workspace: &dyn Workspace,
    primary_path: &Path,
) -> Option<String> {
    let content = read_xml_if_present(workspace, primary_path);
    if content.is_some() {
        archive_xml_file_with_workspace(workspace, primary_path);
    }
    content
}

pub(super) fn parse_issue_location(issue: &str) -> Option<(String, u32, u32)> {
    let location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+):(?P<start>\d+)(?:[-–—](?P<end>\d+))?(?::(?P<col>\d+))?",
    )
    .ok()?;
    let gh_location_re = Regex::new(
        r"(?m)(?P<file>[-_./A-Za-z0-9]+\.[A-Za-z0-9]+)#L(?P<start>\d+)(?:-L(?P<end>\d+))?",
    )
    .ok()?;

    if let Some(cap) = location_re.captures(issue) {
        let file = cap.name("file")?.as_str().to_string();
        let start = cap.name("start")?.as_str().parse::<u32>().ok()?;
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(start);
        return Some((file, start, end));
    }

    if let Some(cap) = gh_location_re.captures(issue) {
        let file = cap.name("file")?.as_str().to_string();
        let start = cap.name("start")?.as_str().parse::<u32>().ok()?;
        let end = cap
            .name("end")
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(start);
        return Some((file, start, end));
    }

    None
}

pub(super) fn read_snippet_for_issue(
    workspace: &dyn Workspace,
    file: &str,
    issue_start: u32,
    issue_end: u32,
) -> Option<XmlCodeSnippet> {
    let issue_start = issue_start.max(1);
    let issue_end = issue_end.max(issue_start);

    let context_lines: u32 = 2;
    let start = issue_start.saturating_sub(context_lines).max(1);
    let end = issue_end.saturating_add(context_lines);

    let content = workspace.read(Path::new(file)).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let max_line = u32::try_from(lines.len()).ok()?;
    let end = end.min(max_line);
    if start > end {
        return None;
    }

    let mut snippet = String::new();
    for line_no in start..=end {
        let idx = usize::try_from(line_no.saturating_sub(1)).ok()?;
        let line = lines.get(idx).copied().unwrap_or_default();
        snippet.push_str(&format!("{:>4} | {}\n", line_no, line));
    }

    Some(XmlCodeSnippet {
        file: file.to_string(),
        line_start: start,
        line_end: end,
        content: snippet,
    })
}
