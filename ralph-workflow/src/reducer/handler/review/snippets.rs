fn extract_issue_snippets(
    issues: &[String],
    workspace: &dyn crate::workspace::Workspace,
) -> Vec<XmlCodeSnippet> {
    let mut snippets = Vec::new();
    let mut seen = HashSet::new();

    let location_re = issue_location_regex();
    let gh_location_re = issue_gh_location_regex();

    for issue in issues {
        let (file, line_start, line_end) = if let Some(cap) = location_re.captures(issue) {
            let file = cap
                .name("file")
                .map(|m| m.as_str().trim().replace('\\', "/"));
            let start = cap
                .name("start")
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let end = cap
                .name("end")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .or(start);
            (file, start, end)
        } else if let Some(cap) = gh_location_re.captures(issue) {
            let file = cap
                .name("file")
                .map(|m| m.as_str().trim().replace('\\', "/"));
            let start = cap
                .name("start")
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let end = cap
                .name("end")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .or(start);
            (file, start, end)
        } else {
            (None, None, None)
        };

        let Some(file) = file else { continue };
        let Some(start) = line_start else { continue };
        let end = line_end.unwrap_or(start);

        let key = (file.clone(), start, end);
        if !seen.insert(key) {
            continue;
        }

        let content = match workspace.read(Path::new(&file)) {
            Ok(content) => content,
            Err(_) => continue,
        };

        if let Some(snippet) = extract_snippet_lines(&content, start, end) {
            snippets.push(XmlCodeSnippet {
                file,
                line_start: start,
                line_end: end,
                content: snippet,
            });
        }
    }

    snippets
}

fn issue_location_regex() -> &'static Regex {
    static LOCATION_RE: OnceLock<Regex> = OnceLock::new();
    LOCATION_RE.get_or_init(|| {
        Regex::new(
            r"(?m)(?P<file>[A-Za-z0-9 ._\-/\\:]+\.[A-Za-z0-9]+):(?P<start>\d+)(?:[-–—](?P<end>\d+))?(?::(?P<col>\d+))?",
        )
        .unwrap_or_else(|_| Regex::new(r"$^").expect("valid fallback regex"))
    })
}

fn issue_gh_location_regex() -> &'static Regex {
    static GH_LOCATION_RE: OnceLock<Regex> = OnceLock::new();
    GH_LOCATION_RE.get_or_init(|| {
        Regex::new(
            r"(?m)(?P<file>[A-Za-z0-9 ._\-/\\:]+\.[A-Za-z0-9]+)#L(?P<start>\d+)(?:-L(?P<end>\d+))?",
        )
        .unwrap_or_else(|_| Regex::new(r"$^").expect("valid fallback regex"))
    })
}

fn extract_snippet_lines(content: &str, start: u32, end: u32) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let start_idx = start.saturating_sub(1) as usize;
    if start_idx >= lines.len() {
        return None;
    }

    let end_idx = end.saturating_sub(1) as usize;
    let end_idx = end_idx.min(lines.len().saturating_sub(1));
    let mut out = String::new();
    for (offset, line) in lines[start_idx..=end_idx].iter().enumerate() {
        let line_no = start + offset as u32;
        out.push_str(&format!("{line_no} | {line}\n"));
    }
    Some(out.trim_end().to_string())
}

fn render_issues_markdown(
    elements: &crate::files::llm_output_extraction::IssuesElements,
) -> String {
    let mut output = String::from("# Issues\n\n");

    if let Some(message) = &elements.no_issues_found {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            output.push_str("No issues found.\n");
        } else {
            output.push_str(trimmed);
            output.push('\n');
        }
        return output;
    }

    if elements.issues.is_empty() {
        output.push_str("No issues found.\n");
        return output;
    }

    for issue in &elements.issues {
        let trimmed = issue.trim();
        if trimmed.is_empty() {
            continue;
        }
        output.push_str("- [ ] ");
        output.push_str(trimmed);
        output.push('\n');
    }

    output
}
