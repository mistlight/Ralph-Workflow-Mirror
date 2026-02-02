enum CommitExtractionOutcome {
    MissingFile(String),
    InvalidXml(String),
    Valid(CommitExtractionResult),
}

fn extract_commit_message_from_file_with_workspace(
    workspace: &dyn Workspace,
) -> CommitExtractionOutcome {
    let Some(xml) =
        try_extract_from_file_with_workspace(workspace, Path::new(xml_paths::COMMIT_MESSAGE_XML))
    else {
        return CommitExtractionOutcome::MissingFile(
            "XML output missing or invalid; agent must write .agent/tmp/commit_message.xml"
                .to_string(),
        );
    };

    let (message, detail) = try_extract_xml_commit_with_trace(&xml);
    match message {
        Some(msg) => CommitExtractionOutcome::Valid(CommitExtractionResult::new(msg)),
        None => CommitExtractionOutcome::InvalidXml(detail),
    }
}
