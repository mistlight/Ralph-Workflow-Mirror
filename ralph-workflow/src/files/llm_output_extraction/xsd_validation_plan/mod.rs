//! XSD validation for plan XML format (v2 - Structured).
//!
//! This module provides validation of structured XML output against the XSD schema
//! to ensure AI agent output conforms to the expected format for development plans.
//!
//! The v2 schema enforces:
//! - Quantified scope items (minimum 3)
//! - Explicit step numbers with types and priorities
//! - Rich content elements (tables, code blocks, lists)
//! - Required risk/mitigation pairs
//! - Required verification strategies

use crate::files::llm_output_extraction::xsd_validation::{XsdErrorType, XsdValidationError};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════════
// RICH CONTENT TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Inline text element (emphasis, code, link, or plain text)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineElement {
    Text(String),
    Emphasis(String),
    Code(String),
    Link { href: String, text: String },
}

/// Paragraph with mixed inline content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    pub content: Vec<InlineElement>,
}

/// Code block with optional language and filename
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub content: String,
    pub language: Option<String>,
    pub filename: Option<String>,
}

/// Table cell with inline content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub content: Vec<InlineElement>,
}

/// Table row containing cells
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<Cell>,
}

/// Table with optional caption and column headers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub caption: Option<String>,
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

/// List type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    Ordered,
    Unordered,
}

/// List item which can contain inline content and nested lists
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub content: Vec<InlineElement>,
    pub nested_list: Option<Box<List>>,
}

/// List container (ordered or unordered)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub list_type: ListType,
    pub items: Vec<ListItem>,
}

/// Heading with level (2-4)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

/// Rich content element - one of the supported content types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentElement {
    Paragraph(Paragraph),
    CodeBlock(CodeBlock),
    Table(Table),
    List(List),
    Heading(Heading),
}

/// Rich content container holding multiple content elements
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichContent {
    pub elements: Vec<ContentElement>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// SCOPE AND SUMMARY TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Scope item with optional count and category for quantification
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeItem {
    pub description: String,
    pub count: Option<String>,
    pub category: Option<String>,
}

/// Plan summary with context and scope items
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSummary {
    pub context: String,
    pub scope_items: Vec<ScopeItem>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// STEP AND FILE TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// File action type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

impl FileAction {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "create" => Some(FileAction::Create),
            "modify" => Some(FileAction::Modify),
            "delete" => Some(FileAction::Delete),
            _ => None,
        }
    }
}

/// Step type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepType {
    #[default]
    FileChange,
    Action,
    Research,
}

impl StepType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "file-change" => Some(StepType::FileChange),
            "action" => Some(StepType::Action),
            "research" => Some(StepType::Research),
            _ => None,
        }
    }
}

/// Priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

impl Priority {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "critical" => Some(Priority::Critical),
            "high" => Some(Priority::High),
            "medium" => Some(Priority::Medium),
            "low" => Some(Priority::Low),
            _ => None,
        }
    }
}

/// Target file in a step
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetFile {
    pub path: String,
    pub action: FileAction,
}

/// Implementation step
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub number: u32,
    pub step_type: StepType,
    pub priority: Option<Priority>,
    pub title: String,
    pub target_files: Vec<TargetFile>,
    pub location: Option<String>,
    pub rationale: Option<String>,
    pub content: RichContent,
    pub depends_on: Vec<u32>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITICAL FILES TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Primary file entry with action and estimated changes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryFile {
    pub path: String,
    pub action: FileAction,
    pub estimated_changes: Option<String>,
}

/// Reference file entry with purpose
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFile {
    pub path: String,
    pub purpose: String,
}

/// Critical files section
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticalFiles {
    pub primary_files: Vec<PrimaryFile>,
    pub reference_files: Vec<ReferenceFile>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// RISKS AND VERIFICATION TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Severity::Low),
            "medium" => Some(Severity::Medium),
            "high" => Some(Severity::High),
            "critical" => Some(Severity::Critical),
            _ => None,
        }
    }
}

/// Risk-mitigation pair
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskPair {
    pub severity: Option<Severity>,
    pub risk: String,
    pub mitigation: String,
}

/// Verification item
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub method: String,
    pub expected_outcome: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// PLAN ELEMENTS (ROOT)
// ═══════════════════════════════════════════════════════════════════════════════

/// Parsed plan elements from valid XML (v2 - structured)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanElements {
    /// The plan summary with context and scope items
    pub summary: PlanSummary,
    /// Implementation steps
    pub steps: Vec<Step>,
    /// Critical files
    pub critical_files: CriticalFiles,
    /// Risks and mitigations
    pub risks_mitigations: Vec<RiskPair>,
    /// Verification strategy
    pub verification_strategy: Vec<Verification>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// XML PARSING HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Extract attributes from a quick-xml BytesStart
fn get_attributes(e: &quick_xml::events::BytesStart) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    for attr in e.attributes().flatten() {
        if let (Ok(key), Ok(value)) = (
            std::str::from_utf8(attr.key.as_ref()),
            std::str::from_utf8(&attr.value),
        ) {
            attrs.insert(key.to_string(), value.to_string());
        }
    }
    attrs
}

/// Read text content until the end tag
fn read_text_until_end(
    reader: &mut Reader<&[u8]>,
    end_tag: &[u8],
) -> Result<String, XsdValidationError> {
    let mut buf = Vec::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(e)) if e.name().as_ref() == end_tag => {
                break;
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: format!("closing </{}>", String::from_utf8_lossy(end_tag)),
                    found: "end of file".to_string(),
                    suggestion: "Check XML is well-formed".to_string(),
                    example: None,
                });
            }
            Ok(_) => {} // Skip other events
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    Ok(text.trim().to_string())
}

/// Skip until the end of the current element (handles nested elements)
fn skip_to_end(reader: &mut Reader<&[u8]>, end_tag: &[u8]) -> Result<(), XsdValidationError> {
    let mut buf = Vec::new();
    let mut depth = 1;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == end_tag => {
                depth += 1;
            }
            Ok(Event::End(e)) if e.name().as_ref() == end_tag => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: format!("closing </{}>", String::from_utf8_lossy(end_tag)),
                    found: "end of file".to_string(),
                    suggestion: "Check XML is well-formed".to_string(),
                    example: None,
                });
            }
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    Ok(())
}

/// Read all content (including nested XML) as a string until end tag
fn read_inner_xml(
    reader: &mut Reader<&[u8]>,
    end_tag: &[u8],
) -> Result<String, XsdValidationError> {
    let mut buf = Vec::new();
    let mut content = String::new();
    let mut depth = 1;

    loop {
        let event = reader.read_event_into(&mut buf);
        match &event {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == end_tag {
                    depth += 1;
                }
                // Reconstruct the tag
                content.push('<');
                content.push_str(&String::from_utf8_lossy(e.name().as_ref()));
                for attr in e.attributes().flatten() {
                    content.push(' ');
                    content.push_str(&String::from_utf8_lossy(attr.key.as_ref()));
                    content.push_str("=\"");
                    content.push_str(&String::from_utf8_lossy(&attr.value));
                    content.push('"');
                }
                content.push('>');
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == end_tag {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                content.push_str("</");
                content.push_str(&String::from_utf8_lossy(e.name().as_ref()));
                content.push('>');
            }
            Ok(Event::Empty(e)) => {
                content.push('<');
                content.push_str(&String::from_utf8_lossy(e.name().as_ref()));
                for attr in e.attributes().flatten() {
                    content.push(' ');
                    content.push_str(&String::from_utf8_lossy(attr.key.as_ref()));
                    content.push_str("=\"");
                    content.push_str(&String::from_utf8_lossy(&attr.value));
                    content.push('"');
                }
                content.push_str("/>");
            }
            Ok(Event::Text(e)) => {
                content.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::CData(e)) => {
                content.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            Ok(Event::Eof) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: format!("closing </{}>", String::from_utf8_lossy(end_tag)),
                    found: "end of file".to_string(),
                    suggestion: "Check XML is well-formed".to_string(),
                    example: None,
                });
            }
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: String::from_utf8_lossy(end_tag).to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    Ok(content)
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION PARSERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Strip block-level elements from content for inline parsing.
///
/// This allows list items to contain block-level elements like `<code-block>`,
/// `<paragraph>`, and nested `<list>` without breaking inline parsing.
/// The block elements are removed, leaving only inline content to be parsed.
/// Strip block-level elements from content for inline parsing.
///
/// This allows list items to contain block-level elements like `<code-block>`,
/// `<paragraph>`, and nested `<list>` without breaking inline parsing.
/// The block elements are removed, leaving only inline content to be parsed.
// pub(super) for test access from tests submodule
pub(super) fn strip_block_elements_for_inline_parsing(content: &str) -> String {
    let mut result = content.to_string();

    // Remove <list>...</list> blocks (handled separately for nesting)
    while let Some(start) = result.find("<list") {
        if let Some(end) = result[start..].find("</list>") {
            result = format!("{}{}", &result[..start], &result[start + end + 7..]);
        } else {
            break;
        }
    }

    // Remove <code-block>...</code-block> blocks
    while let Some(start) = result.find("<code-block") {
        if let Some(end) = result[start..].find("</code-block>") {
            result = format!("{}{}", &result[..start], &result[start + end + 13..]);
        } else {
            break;
        }
    }

    // Remove <paragraph>...</paragraph> blocks
    while let Some(start) = result.find("<paragraph") {
        if let Some(end) = result[start..].find("</paragraph>") {
            result = format!("{}{}", &result[..start], &result[start + end + 12..]);
        } else {
            break;
        }
    }

    result
}

/// Parse inline content elements (text, emphasis, code, link)
fn parse_inline_elements(content: &str) -> Vec<InlineElement> {
    let mut elements = Vec::new();
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                // Flush any accumulated text
                if !current_text.trim().is_empty() {
                    elements.push(InlineElement::Text(current_text.trim().to_string()));
                }
                current_text.clear();

                match e.name().as_ref() {
                    b"emphasis" => {
                        if let Ok(text) = read_text_until_end(&mut reader, b"emphasis") {
                            elements.push(InlineElement::Emphasis(text));
                        }
                    }
                    b"code" => {
                        if let Ok(text) = read_text_until_end(&mut reader, b"code") {
                            elements.push(InlineElement::Code(text));
                        }
                    }
                    b"link" => {
                        let attrs = get_attributes(&e);
                        let href = attrs.get("href").cloned().unwrap_or_default();
                        if let Ok(text) = read_text_until_end(&mut reader, b"link") {
                            elements.push(InlineElement::Link { href, text });
                        }
                    }
                    _ => {
                        // Skip unknown inline elements
                        let _ = skip_to_end(&mut reader, e.name().as_ref());
                    }
                }
            }
            Ok(Event::Text(e)) => {
                current_text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(_)) | Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    // Flush any remaining text
    if !current_text.trim().is_empty() {
        elements.push(InlineElement::Text(current_text.trim().to_string()));
    }

    elements
}

/// Parse rich content from a <content> element
fn parse_rich_content(content: &str) -> Result<RichContent, XsdValidationError> {
    let mut elements = Vec::new();
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"paragraph" => {
                        let inner = read_inner_xml(&mut reader, b"paragraph")?;
                        elements.push(ContentElement::Paragraph(Paragraph {
                            content: parse_inline_elements(&inner),
                        }));
                    }
                    b"code-block" => {
                        let attrs = get_attributes(&e);
                        let code = read_text_until_end(&mut reader, b"code-block")?;
                        elements.push(ContentElement::CodeBlock(CodeBlock {
                            content: code,
                            language: attrs.get("language").cloned(),
                            filename: attrs.get("filename").cloned(),
                        }));
                    }
                    b"heading" => {
                        let attrs = get_attributes(&e);
                        let level: u8 =
                            attrs.get("level").and_then(|s| s.parse().ok()).unwrap_or(3);
                        let text = read_text_until_end(&mut reader, b"heading")?;
                        if !(2..=4).contains(&level) {
                            return Err(XsdValidationError {
                                error_type: XsdErrorType::InvalidContent,
                                element_path: "heading/@level".to_string(),
                                expected: "level between 2 and 4".to_string(),
                                found: level.to_string(),
                                suggestion: "Use level=\"2\", level=\"3\", or level=\"4\""
                                    .to_string(),
                                example: None,
                            });
                        }
                        elements.push(ContentElement::Heading(Heading { level, text }));
                    }
                    b"list" => {
                        let attrs = get_attributes(&e);
                        let list_type_str = attrs.get("type").map(|s| s.as_str()).unwrap_or("");
                        let list_type = match list_type_str {
                            "ordered" => ListType::Ordered,
                            "unordered" => ListType::Unordered,
                            _ => {
                                return Err(XsdValidationError {
                                    error_type: XsdErrorType::InvalidContent,
                                    element_path: "list/@type".to_string(),
                                    expected: "ordered or unordered".to_string(),
                                    found: list_type_str.to_string(),
                                    suggestion: "Use type=\"ordered\" or type=\"unordered\""
                                        .to_string(),
                                    example: None,
                                });
                            }
                        };
                        let list = parse_list(&mut reader, list_type)?;
                        elements.push(ContentElement::List(list));
                    }
                    b"table" => {
                        let table = parse_table(&mut reader)?;
                        elements.push(ContentElement::Table(table));
                    }
                    _ => {
                        // Skip unknown elements
                        let _ = skip_to_end(&mut reader, e.name().as_ref());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "content".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if elements.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "content".to_string(),
            expected: "at least one content element".to_string(),
            found: "empty content".to_string(),
            suggestion: "Add <paragraph>, <code-block>, <table>, <list>, or <heading> elements"
                .to_string(),
            example: None,
        });
    }

    Ok(RichContent { elements })
}

/// Parse a list element
fn parse_list(reader: &mut Reader<&[u8]>, list_type: ListType) -> Result<List, XsdValidationError> {
    let mut items = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"item" => {
                let inner = read_inner_xml(reader, b"item")?;
                // Check for nested list
                let nested_list = if inner.contains("<list") {
                    // Parse the nested list separately
                    let mut inner_reader = Reader::from_str(&inner);
                    inner_reader.config_mut().trim_text(true);
                    let mut inner_buf = Vec::new();
                    let mut nested = None;

                    loop {
                        match inner_reader.read_event_into(&mut inner_buf) {
                            Ok(Event::Start(e2)) if e2.name().as_ref() == b"list" => {
                                let attrs = get_attributes(&e2);
                                let nested_type =
                                    match attrs.get("type").map(|s| s.as_str()).unwrap_or("") {
                                        "ordered" => ListType::Ordered,
                                        "unordered" => ListType::Unordered,
                                        _ => ListType::Unordered,
                                    };
                                nested =
                                    Some(Box::new(parse_list(&mut inner_reader, nested_type)?));
                            }
                            Ok(Event::Eof) => break,
                            Ok(_) => {}
                            Err(_) => break,
                        }
                        inner_buf.clear();
                    }
                    nested
                } else {
                    None
                };

                // Extract text content, stripping out block-level elements that we allow
                // but don't need to parse into the data structure (code-block, paragraph, list)
                let text_content = strip_block_elements_for_inline_parsing(&inner);

                items.push(ListItem {
                    content: parse_inline_elements(&text_content),
                    nested_list,
                });
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"list" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "list".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if items.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "list".to_string(),
            expected: "at least one <item> element".to_string(),
            found: "empty list".to_string(),
            suggestion: "Add <item>...</item> to the list".to_string(),
            example: None,
        });
    }

    Ok(List { list_type, items })
}

/// Parse a table element
fn parse_table(reader: &mut Reader<&[u8]>) -> Result<Table, XsdValidationError> {
    let mut caption = None;
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"caption" => {
                    caption = Some(read_text_until_end(reader, b"caption")?);
                }
                b"columns" => {
                    columns = parse_columns(reader)?;
                }
                b"row" => {
                    rows.push(parse_row(reader)?);
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"table" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "table".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if rows.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "table".to_string(),
            expected: "at least one <row> element".to_string(),
            found: "no rows".to_string(),
            suggestion: "Add <row><cell>...</cell></row> to the table".to_string(),
            example: None,
        });
    }

    Ok(Table {
        caption,
        columns,
        rows,
    })
}

/// Parse table columns
fn parse_columns(reader: &mut Reader<&[u8]>) -> Result<Vec<String>, XsdValidationError> {
    let mut columns = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"column" => {
                columns.push(read_text_until_end(reader, b"column")?);
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"columns" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    Ok(columns)
}

/// Parse a table row
fn parse_row(reader: &mut Reader<&[u8]>) -> Result<Row, XsdValidationError> {
    let mut cells = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"cell" => {
                let inner = read_inner_xml(reader, b"cell")?;
                cells.push(Cell {
                    content: parse_inline_elements(&inner),
                });
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"row" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    if cells.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "table/row".to_string(),
            expected: "at least one <cell> in each row".to_string(),
            found: "empty row".to_string(),
            suggestion: "Add <cell> elements to the row".to_string(),
            example: None,
        });
    }

    Ok(Row { cells })
}

/// Parse the ralph-summary section
fn parse_summary(reader: &mut Reader<&[u8]>) -> Result<PlanSummary, XsdValidationError> {
    let mut context = None;
    let mut scope_items = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"context" => {
                    context = Some(read_text_until_end(reader, b"context")?);
                }
                b"scope-items" => {
                    scope_items = parse_scope_items(reader)?;
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-summary" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-summary".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    let context = context.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-summary/context".to_string(),
        expected: "<context> element".to_string(),
        found: "no <context> found".to_string(),
        suggestion: "Add <context>Description of what is being done</context>".to_string(),
        example: None,
    })?;

    if context.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-summary/context".to_string(),
            expected: "non-empty context".to_string(),
            found: "empty context".to_string(),
            suggestion: "Provide a description of what is being done".to_string(),
            example: None,
        });
    }

    if scope_items.len() < 3 {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-summary/scope-items".to_string(),
            expected: "at least 3 scope-item elements".to_string(),
            found: format!("{} scope-item(s)", scope_items.len()),
            suggestion:
                "Add more <scope-item count=\"N\" category=\"X\">description</scope-item> elements"
                    .to_string(),
            example: None,
        });
    }

    Ok(PlanSummary {
        context,
        scope_items,
    })
}

/// Parse scope-items
fn parse_scope_items(reader: &mut Reader<&[u8]>) -> Result<Vec<ScopeItem>, XsdValidationError> {
    let mut items = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"scope-item" => {
                let attrs = get_attributes(&e);
                let description = read_text_until_end(reader, b"scope-item")?;
                items.push(ScopeItem {
                    description,
                    count: attrs.get("count").cloned(),
                    category: attrs.get("category").cloned(),
                });
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"scope-items" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    Ok(items)
}

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
                    found: format!("parse error: {}", e),
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

    let step_type = attrs
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
                    element_path: format!("step[{}]", number),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    let title = title.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("step[{}]/title", number),
        expected: "<title> element".to_string(),
        found: "no <title> found".to_string(),
        suggestion: "Add <title>Step title</title>".to_string(),
        example: None,
    })?;

    // Validate file-change steps have target-files
    if step_type == StepType::FileChange && target_files.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: format!("step[{}]/target-files", number),
            expected: "<target-files> with at least one <file> for file-change steps".to_string(),
            found: "no target-files".to_string(),
            suggestion: "Add <target-files><file path=\"...\" action=\"modify\"/></target-files>"
                .to_string(),
            example: None,
        });
    }

    let content = content.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("step[{}]/content", number),
        expected: "<content> element".to_string(),
        found: "no <content> found".to_string(),
        suggestion: "Add <content><paragraph>...</paragraph></content>".to_string(),
        example: None,
    })?;

    Ok(Step {
        number,
        step_type,
        priority,
        title,
        target_files,
        location,
        rationale,
        content,
        depends_on,
    })
}

/// Helper to parse a single <file> element's attributes into a TargetFile
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
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
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
                    found: format!("parse error: {}", e),
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
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"file" => {
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
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
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
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"file" => {
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
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    Ok(files)
}

/// Parse the ralph-risks-mitigations section
fn parse_risks_mitigations(
    reader: &mut Reader<&[u8]>,
) -> Result<Vec<RiskPair>, XsdValidationError> {
    let mut risk_pairs = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"risk-pair" => {
                let attrs = get_attributes(&e);
                let severity = attrs.get("severity").and_then(|s| Severity::from_str(s));
                let pair = parse_risk_pair(reader, severity)?;
                risk_pairs.push(pair);
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-risks-mitigations" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-risks-mitigations".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if risk_pairs.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-risks-mitigations".to_string(),
            expected: "at least one <risk-pair> element".to_string(),
            found: "no risk-pairs".to_string(),
            suggestion:
                "Add <risk-pair severity=\"medium\"><risk>...</risk><mitigation>...</mitigation></risk-pair>"
                    .to_string(),
                    example: None,
        });
    }

    Ok(risk_pairs)
}

/// Parse a single risk-pair
fn parse_risk_pair(
    reader: &mut Reader<&[u8]>,
    severity: Option<Severity>,
) -> Result<RiskPair, XsdValidationError> {
    let mut risk = None;
    let mut mitigation = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"risk" => {
                    risk = Some(read_text_until_end(reader, b"risk")?);
                }
                b"mitigation" => {
                    mitigation = Some(read_text_until_end(reader, b"mitigation")?);
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"risk-pair" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    let risk = risk.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "risk-pair/risk".to_string(),
        expected: "<risk> element".to_string(),
        found: "no <risk> found".to_string(),
        suggestion: "Add <risk>Risk description</risk>".to_string(),
        example: None,
    })?;

    let mitigation = mitigation.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "risk-pair/mitigation".to_string(),
        expected: "<mitigation> element".to_string(),
        found: "no <mitigation> found".to_string(),
        suggestion: "Add <mitigation>How to mitigate</mitigation>".to_string(),
        example: None,
    })?;

    Ok(RiskPair {
        severity,
        risk,
        mitigation,
    })
}

/// Parse the ralph-verification-strategy section
fn parse_verification_strategy(
    reader: &mut Reader<&[u8]>,
) -> Result<Vec<Verification>, XsdValidationError> {
    let mut verifications = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"verification" => {
                verifications.push(parse_single_verification(reader)?);
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-verification-strategy" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-verification-strategy".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if verifications.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-verification-strategy".to_string(),
            expected: "at least one <verification> element".to_string(),
            found: "no verifications".to_string(),
            suggestion:
                "Add <verification><method>...</method><expected-outcome>...</expected-outcome></verification>"
                    .to_string(),
            example: None,
        });
    }

    Ok(verifications)
}

/// Parse a single verification element
fn parse_single_verification(
    reader: &mut Reader<&[u8]>,
) -> Result<Verification, XsdValidationError> {
    let mut method = None;
    let mut expected_outcome = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"method" => {
                    method = Some(read_text_until_end(reader, b"method")?);
                }
                b"expected-outcome" => {
                    expected_outcome = Some(read_text_until_end(reader, b"expected-outcome")?);
                }
                _ => {
                    let _ = skip_to_end(reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"verification" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }

    let method = method.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "verification/method".to_string(),
        expected: "<method> element".to_string(),
        found: "no <method> found".to_string(),
        suggestion: "Add <method>How to verify</method>".to_string(),
        example: None,
    })?;

    let expected_outcome = expected_outcome.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "verification/expected-outcome".to_string(),
        expected: "<expected-outcome> element".to_string(),
        found: "no <expected-outcome> found".to_string(),
        suggestion: "Add <expected-outcome>What success looks like</expected-outcome>".to_string(),
        example: None,
    })?;

    Ok(Verification {
        method,
        expected_outcome,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// MAIN VALIDATION FUNCTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Validate plan XML content against the structured XSD schema.
///
/// This validates that the XML content conforms to the expected
/// structured plan format with rich content elements.
///
/// # Arguments
///
/// * `xml_content` - The XML content to validate
///
/// # Returns
///
/// * `Ok(PlanElements)` if the XML is valid and contains all required elements
/// * `Err(XsdValidationError)` if the XML is invalid or doesn't conform to the schema
pub fn validate_plan_xml(xml_content: &str) -> Result<PlanElements, XsdValidationError> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut summary = None;
    let mut steps = None;
    let mut critical_files = None;
    let mut risks_mitigations = None;
    let mut verification_strategy = None;
    let mut found_root = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"ralph-plan" => {
                    found_root = true;
                }
                b"ralph-summary" if found_root => {
                    summary = Some(parse_summary(&mut reader)?);
                }
                b"ralph-implementation-steps" if found_root => {
                    steps = Some(parse_steps(&mut reader)?);
                }
                b"ralph-critical-files" if found_root => {
                    critical_files = Some(parse_critical_files(&mut reader)?);
                }
                b"ralph-risks-mitigations" if found_root => {
                    risks_mitigations = Some(parse_risks_mitigations(&mut reader)?);
                }
                b"ralph-verification-strategy" if found_root => {
                    verification_strategy = Some(parse_verification_strategy(&mut reader)?);
                }
                _ => {
                    // Skip unknown elements
                    let _ = skip_to_end(&mut reader, e.name().as_ref());
                }
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"ralph-plan" => break,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(XsdValidationError {
                    error_type: XsdErrorType::MalformedXml,
                    element_path: "ralph-plan".to_string(),
                    expected: "valid XML".to_string(),
                    found: format!("parse error: {}", e),
                    suggestion: "Check XML syntax".to_string(),
                    example: None,
                });
            }
        }
        buf.clear();
    }

    if !found_root {
        return Err(XsdValidationError {
            error_type: XsdErrorType::MissingRequiredElement,
            element_path: "ralph-plan".to_string(),
            expected: "<ralph-plan> as root element".to_string(),
            found: "no <ralph-plan> found".to_string(),
            suggestion: "Wrap your plan in <ralph-plan> tags".to_string(),
            example: None,
        });
    }

    let summary = summary.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-summary".to_string(),
        expected: "<ralph-summary> element".to_string(),
        found: "no <ralph-summary> found".to_string(),
        suggestion:
            "Add <ralph-summary><context>...</context><scope-items>...</scope-items></ralph-summary>"
                .to_string(),
        example: None,
    })?;

    let steps = steps.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-implementation-steps".to_string(),
        expected: "<ralph-implementation-steps> element".to_string(),
        found: "no <ralph-implementation-steps> found".to_string(),
        suggestion: "Add <ralph-implementation-steps><step>...</step></ralph-implementation-steps>"
            .to_string(),
        example: None,
    })?;

    let critical_files = critical_files.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-critical-files".to_string(),
        expected: "<ralph-critical-files> element".to_string(),
        found: "no <ralph-critical-files> found".to_string(),
        suggestion:
            "Add <ralph-critical-files><primary-files>...</primary-files></ralph-critical-files>"
                .to_string(),
        example: None,
    })?;

    let risks_mitigations = risks_mitigations.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-risks-mitigations".to_string(),
        expected: "<ralph-risks-mitigations> element".to_string(),
        found: "no <ralph-risks-mitigations> found".to_string(),
        suggestion:
            "Add <ralph-risks-mitigations><risk-pair>...</risk-pair></ralph-risks-mitigations>"
                .to_string(),
        example: None,
    })?;

    let verification_strategy = verification_strategy.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-verification-strategy".to_string(),
        expected: "<ralph-verification-strategy> element".to_string(),
        found: "no <ralph-verification-strategy> found".to_string(),
        suggestion: "Add <ralph-verification-strategy><verification>...</verification></ralph-verification-strategy>".to_string(),
        example: None,
    })?;

    Ok(PlanElements {
        summary,
        steps,
        critical_files,
        risks_mitigations,
        verification_strategy,
    })
}

#[cfg(test)]
mod tests;
