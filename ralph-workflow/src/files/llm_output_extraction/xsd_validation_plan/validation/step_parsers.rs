// Step parsing functions (parse_steps, parse_single_step, parse_file_element, parse_target_files, parse_critical_files)

/// Parse the ralph-implementation-steps section
fn parse_steps(reader: &mut Reader<&[u8]>) -> Result<Vec<Step>, XsdValidationError> {
    let mut steps = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"step" => {
                let attrs = get_attributes(&e);
                let step = parse_single_step(reader, &attrs)?;
                steps.push(step);
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-implementation-steps" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-implementation-steps".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {e}"),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if steps.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-implementation-steps".to_string(),
            expected: "at least one <step> element".to_string(),
            found: "no steps".to_string(),
            suggestion: "Add <step number=\"1\">...</step>".to_string(),
            example: None,
        });
    }

    Ok(steps)
}

/// Parse a single step element
fn parse_single_step(
    reader: &mut Reader<&[u8]>,
    attrs: &HashMap<String, String>,
) -> Result<Step, XsdValidationError> {
    let number: u32 = attrs
        .get("number")
        .ok_or_else(|| XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "step".to_string(),
            expected: "number attribute".to_string(),
            found: "no number attribute".to_string(),
            suggestion: "Add number=\"N\" to the step".to_string(),
            example: None,
        })?
        .parse()
        .map_err(|_| XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "step/@number".to_string(),
            expected: "positive integer".to_string(),
            found: attrs.get("number").cloned().unwrap_or_default(),
            suggestion: "Use a positive integer for step number".to_string(),
            example: None,
        })?;

    let kind = attrs
        .get("type")
        .and_then(|s| StepType::from_str(s))
        .unwrap_or_default();

    let priority = attrs.get("priority").and_then(|s| Priority::from_str(s));

    let mut title = None;
    let mut target_files = Vec::new();
    let mut location = None;
    let mut rationale = None;
    let mut content = None;
    let mut depends_on = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"title" => {
                    title = Some(read_text_until_end(reader, b"title")?);
                }
                b"target-files" => {
                    target_files = parse_target_files(reader)?;
                }
                b"location" => {
                    location = Some(read_text_until_end(reader, b"location")?);
                }
                b"rationale" => {
                    rationale = Some(read_text_until_end(reader, b"rationale")?);
                }
                b"content" => {
                    let inner = read_inner_xml(reader, b"content")?;
                    content = Some(parse_rich_content(&inner)?);
                }
                b"depends-on" => {
                    let dep_attrs = get_attributes(&e);
                    if let Some(step_num) = dep_attrs.get("step").and_then(|s| s.parse().ok()) {
                        depends_on.push(step_num);
                    }
                    let _ = skip_to_end(reader, b"depends-on");
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::Empty(e)) if e.name().as_ref() == b"depends-on" => {
                let dep_attrs = get_attributes(&e);
                if let Some(step_num) = dep_attrs.get("step").and_then(|s| s.parse().ok()) {
                    depends_on.push(step_num);
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"step" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: format!("step[{number}]"),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {e}"),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    let title = title.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("step[{number}]/title"),
        expected: "<title> element".to_string(),
        found: "no <title> found".to_string(),
        suggestion: "Add <title>Step title</title>".to_string(),
        example: None,
    })?;

    // Validate file-change steps have target-files
    if kind == StepType::FileChange && target_files.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: format!("step[{number}]/target-files"),
            expected: "<target-files> with at least one <file> for file-change steps".to_string(),
            found: "no target-files".to_string(),
            suggestion: "Add <target-files><file path=\"...\" action=\"modify\"/></target-files>"
                .to_string(),
            example: None,
        });
    }

    let content = content.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("step[{number}]/content"),
        expected: "<content> element".to_string(),
        found: "no <content> found".to_string(),
        suggestion: "Add <content><paragraph>...</paragraph></content>".to_string(),
        example: None,
    })?;

    Ok(Step {
        number,
        kind,
        priority,
        title,
        target_files,
        location,
        rationale,
        content,
        depends_on,
    })
}

/// Helper to parse a single <file> element's attributes into a `TargetFile`
fn parse_file_element(attrs: &HashMap<String, String>) -> Result<TargetFile, XsdValidationError> {
    let path = attrs
        .get("path")
        .cloned()
        .ok_or_else(|| XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "target-files/file".to_string(),
            expected: "path attribute".to_string(),
            found: "no path attribute".to_string(),
            suggestion: "Add path=\"...\" to the file element".to_string(),
            example: None,
        })?;

    let action_str = attrs
        .get("action")
        .cloned()
        .ok_or_else(|| XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "target-files/file".to_string(),
            expected: "action attribute".to_string(),
            found: "no action attribute".to_string(),
            suggestion: "Add action=\"create|modify|delete\" to the file element".to_string(),
            example: None,
        })?;

    let action = FileAction::from_str(&action_str).ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::InvalidContent,
        element_path: "target-files/file/@action".to_string(),
        expected: "create, modify, or delete".to_string(),
        found: action_str,
        suggestion: "Use action=\"create\", action=\"modify\", or action=\"delete\"".to_string(),
        example: None,
    })?;

    Ok(TargetFile { path, action })
}

/// Parse target-files
fn parse_target_files(reader: &mut Reader<&[u8]>) -> Result<Vec<TargetFile>, XsdValidationError> {
    let mut files = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"file" {
                    let attrs = get_attributes(&e);
                    let file = parse_file_element(&attrs)?;
                    files.push(file);
                    // Skip to </file> end tag
                    let _ = skip_to_end(reader, b"file");
                }
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == b"file" => {
                let attrs = get_attributes(&e);
                let file = parse_file_element(&attrs)?;
                files.push(file);
                // No need to skip - self-closing tag has no end
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"target-files" => break,
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
        buf.clear();
    }

    Ok(files)
}

/// Parse the ralph-critical-files section
fn parse_critical_files(reader: &mut Reader<&[u8]>) -> Result<CriticalFiles, XsdValidationError> {
    let mut primary_files = Vec::new();
    let mut reference_files = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"primary-files" => {
                    primary_files = parse_primary_files(reader)?;
                }
                b"reference-files" => {
                    reference_files = parse_reference_files(reader)?;
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-critical-files" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-critical-files".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {e}"),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if primary_files.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-critical-files/primary-files".to_string(),
            expected: "at least one <file> element".to_string(),
            found: "no files".to_string(),
            suggestion: "Add <file path=\"...\" action=\"modify\"/> to primary-files".to_string(),
            example: None,
        });
    }

    Ok(CriticalFiles {
        primary_files,
        reference_files,
    })
}

/// Parse primary-files
fn parse_primary_files(reader: &mut Reader<&[u8]>) -> Result<Vec<PrimaryFile>, XsdValidationError> {
    let mut files = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"file" => {
                let attrs = get_attributes(&e);
                let path = attrs
                    .get("path")
                    .cloned()
                    .ok_or_else(|| XsdValidationError {
                        error_type: XsdErrorType::MissingRequiredElement,
                        element_path: "primary-files/file".to_string(),
                        expected: "path attribute".to_string(),
                        found: "no path attribute".to_string(),
                        suggestion: "Add path=\"...\" to the file element".to_string(),
                        example: None,
                    })?;

                let action_str =
                    attrs
                        .get("action")
                        .cloned()
                        .ok_or_else(|| XsdValidationError {
                            error_type: XsdErrorType::MissingRequiredElement,
                            element_path: "primary-files/file".to_string(),
                            expected: "action attribute".to_string(),
                            found: "no action attribute".to_string(),
                            suggestion: "Add action=\"create|modify|delete\" to the file element"
                                .to_string(),
                            example: None,
                        })?;

                let action =
                    FileAction::from_str(&action_str).ok_or_else(|| XsdValidationError {
                        error_type: XsdErrorType::InvalidContent,
                        element_path: "primary-files/file/@action".to_string(),
                        expected: "create, modify, or delete".to_string(),
                        found: action_str,
                        suggestion:
                            "Use action=\"create\", action=\"modify\", or action=\"delete\""
                                .to_string(),
                        example: None,
                    })?;

                files.push(PrimaryFile {
                    path,
                    action,
                    estimated_changes: attrs.get("estimated-changes").cloned(),
                });
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"primary-files" => break,
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
        buf.clear();
    }

    Ok(files)
}

/// Parse reference-files
fn parse_reference_files(
    reader: &mut Reader<&[u8]>,
) -> Result<Vec<ReferenceFile>, XsdValidationError> {
    let mut files = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"file" => {
                let attrs = get_attributes(&e);
                let path = attrs
                    .get("path")
                    .cloned()
                    .ok_or_else(|| XsdValidationError {
                        error_type: XsdErrorType::MissingRequiredElement,
                        element_path: "reference-files/file".to_string(),
                        expected: "path attribute".to_string(),
                        found: "no path attribute".to_string(),
                        suggestion: "Add path=\"...\" to the file element".to_string(),
                        example: None,
                    })?;

                let purpose = attrs
                    .get("purpose")
                    .cloned()
                    .ok_or_else(|| XsdValidationError {
                        error_type: XsdErrorType::MissingRequiredElement,
                        element_path: "reference-files/file".to_string(),
                        expected: "purpose attribute".to_string(),
                        found: "no purpose attribute".to_string(),
                        suggestion: "Add purpose=\"...\" to the file element".to_string(),
                        example: None,
                    })?;

                files.push(ReferenceFile { path, purpose });
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"reference-files" => break,
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
        buf.clear();
    }

    Ok(files)
}
