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

                // Extract text content (before any nested list)
                let text_content = if inner.contains("<list") {
                    inner.split("<list").next().unwrap_or(&inner)
                } else {
                    &inner
                };

                items.push(ListItem {
                    content: parse_inline_elements(text_content),
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
    })?;

    if context.is_empty() {
        return Err(XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "ralph-summary/context".to_string(),
            expected: "non-empty context".to_string(),
            found: "empty context".to_string(),
            suggestion: "Provide a description of what is being done".to_string(),
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
        })?
        .parse()
        .map_err(|_| XsdValidationError {
            error_type: XsdErrorType::InvalidContent,
            element_path: "step/@number".to_string(),
            expected: "positive integer".to_string(),
            found: attrs.get("number").cloned().unwrap_or_default(),
            suggestion: "Use a positive integer for step number".to_string(),
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
        });
    }

    let content = content.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: format!("step[{}]/content", number),
        expected: "<content> element".to_string(),
        found: "no <content> found".to_string(),
        suggestion: "Add <content><paragraph>...</paragraph></content>".to_string(),
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
        })?;

    let action = FileAction::from_str(&action_str).ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::InvalidContent,
        element_path: "target-files/file/@action".to_string(),
        expected: "create, modify, or delete".to_string(),
        found: action_str,
        suggestion: "Use action=\"create\", action=\"modify\", or action=\"delete\"".to_string(),
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
    })?;

    let mitigation = mitigation.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "risk-pair/mitigation".to_string(),
        expected: "<mitigation> element".to_string(),
        found: "no <mitigation> found".to_string(),
        suggestion: "Add <mitigation>How to mitigate</mitigation>".to_string(),
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
    })?;

    let expected_outcome = expected_outcome.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "verification/expected-outcome".to_string(),
        expected: "<expected-outcome> element".to_string(),
        found: "no <expected-outcome> found".to_string(),
        suggestion: "Add <expected-outcome>What success looks like</expected-outcome>".to_string(),
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
    })?;

    let steps = steps.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-implementation-steps".to_string(),
        expected: "<ralph-implementation-steps> element".to_string(),
        found: "no <ralph-implementation-steps> found".to_string(),
        suggestion: "Add <ralph-implementation-steps><step>...</step></ralph-implementation-steps>"
            .to_string(),
    })?;

    let critical_files = critical_files.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-critical-files".to_string(),
        expected: "<ralph-critical-files> element".to_string(),
        found: "no <ralph-critical-files> found".to_string(),
        suggestion:
            "Add <ralph-critical-files><primary-files>...</primary-files></ralph-critical-files>"
                .to_string(),
    })?;

    let risks_mitigations = risks_mitigations.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-risks-mitigations".to_string(),
        expected: "<ralph-risks-mitigations> element".to_string(),
        found: "no <ralph-risks-mitigations> found".to_string(),
        suggestion:
            "Add <ralph-risks-mitigations><risk-pair>...</risk-pair></ralph-risks-mitigations>"
                .to_string(),
    })?;

    let verification_strategy = verification_strategy.ok_or_else(|| XsdValidationError {
        error_type: XsdErrorType::MissingRequiredElement,
        element_path: "ralph-verification-strategy".to_string(),
        expected: "<ralph-verification-strategy> element".to_string(),
        found: "no <ralph-verification-strategy> found".to_string(),
        suggestion: "Add <ralph-verification-strategy><verification>...</verification></ralph-verification-strategy>".to_string(),
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
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // MINIMAL VALID PLAN TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_validate_minimal_valid_plan() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Add a new feature to the application</context>
<scope-items>
<scope-item count="3" category="files">files to modify</scope-item>
<scope-item count="1" category="feature">new feature</scope-item>
<scope-item count="5" category="tests">test cases</scope-item>
</scope-items>
</ralph-summary>

<ralph-implementation-steps>
<step number="1" type="file-change" priority="high">
<title>Add configuration</title>
<target-files>
<file path="src/config.rs" action="modify"/>
</target-files>
<location>After the imports</location>
<content>
<paragraph>Add new configuration option.</paragraph>
</content>
</step>
</ralph-implementation-steps>

<ralph-critical-files>
<primary-files>
<file path="src/config.rs" action="modify" estimated-changes="~20 lines"/>
</primary-files>
</ralph-critical-files>

<ralph-risks-mitigations>
<risk-pair severity="low">
<risk>Breaking existing configuration</risk>
<mitigation>Add backward compatibility</mitigation>
</risk-pair>
</ralph-risks-mitigations>

<ralph-verification-strategy>
<verification>
<method>Run unit tests</method>
<expected-outcome>All tests pass</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();
        assert_eq!(plan.summary.scope_items.len(), 3);
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].number, 1);
        assert_eq!(plan.steps[0].step_type, StepType::FileChange);
        assert_eq!(plan.steps[0].priority, Some(Priority::High));
        assert_eq!(plan.critical_files.primary_files.len(), 1);
        assert_eq!(plan.risks_mitigations.len(), 1);
        assert_eq!(plan.verification_strategy.len(), 1);
    }

    #[test]
    fn test_missing_root_element() {
        let xml = "Some random text";
        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().element_path, "ralph-plan");
    }

    #[test]
    fn test_missing_summary() {
        let xml = r#"<ralph-plan>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().element_path, "ralph-summary");
    }

    #[test]
    fn test_insufficient_scope_items() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
<scope-items>
<scope-item>Only one item</scope-item>
<scope-item>Two items</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("scope-items"));
        assert!(err.found.contains("2"));
    }

    #[test]
    fn test_action_step_without_target_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test action step</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Configure environment</title>
<content>
<paragraph>Set up the test environment.</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();
        assert_eq!(plan.steps[0].step_type, StepType::Action);
        assert!(plan.steps[0].target_files.is_empty());
    }

    #[test]
    fn test_file_change_step_requires_target_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test file-change step</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Modify config</title>
<content>
<paragraph>Change the configuration.</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("target-files"));
    }

    #[test]
    fn test_parse_code_block() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test code block</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test step</title>
<content>
<code-block language="rust" filename="test.rs">
fn main() {
    println!("Hello");
}
</code-block>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();
        if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[0] {
            assert_eq!(cb.language, Some("rust".to_string()));
            assert_eq!(cb.filename, Some("test.rs".to_string()));
            assert!(cb.content.contains("println"));
        } else {
            panic!("Expected code block");
        }
    }

    #[test]
    fn test_parse_table() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test table</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test step</title>
<content>
<table>
<caption>Test Table</caption>
<columns>
<column>Name</column>
<column>Value</column>
</columns>
<row>
<cell>foo</cell>
<cell>bar</cell>
</row>
<row>
<cell>baz</cell>
<cell>qux</cell>
</row>
</table>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();
        if let ContentElement::Table(t) = &plan.steps[0].content.elements[0] {
            assert_eq!(t.caption, Some("Test Table".to_string()));
            assert_eq!(t.columns.len(), 2);
            assert_eq!(t.rows.len(), 2);
            assert_eq!(t.rows[0].cells.len(), 2);
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_parse_list() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test list</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test step</title>
<content>
<list type="ordered">
<item>First item</item>
<item>Second item</item>
<item>Third item</item>
</list>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>Test risk</risk><mitigation>Test mitigation</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>Test</method><expected-outcome>Pass</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();
        if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
            assert_eq!(l.list_type, ListType::Ordered);
            assert_eq!(l.items.len(), 3);
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_complex_plan_with_dependencies() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Implement OAuth2 authentication for the application</context>
<scope-items>
<scope-item count="3" category="auth">OAuth2 provider integrations</scope-item>
<scope-item count="5" category="api">new API endpoints</scope-item>
<scope-item count="2" category="ui">login components</scope-item>
<scope-item count="8" category="tests">test cases</scope-item>
</scope-items>
</ralph-summary>

<ralph-implementation-steps>
<step number="1" type="file-change" priority="high">
<title>Add OAuth2 configuration</title>
<target-files>
<file path="src/config/oauth.rs" action="create"/>
<file path="src/config/mod.rs" action="modify"/>
</target-files>
<location>Create new file; add mod to mod.rs</location>
<rationale>Configuration must exist before providers</rationale>
<content>
<paragraph>Create OAuth2 configuration:</paragraph>
<code-block language="rust">
pub struct OAuth2Config {
    pub client_id: String,
}
</code-block>
</content>
</step>

<step number="2" type="research" priority="medium">
<title>Research OAuth2 libraries</title>
<content>
<paragraph>Evaluate libraries:</paragraph>
<table>
<caption>Library Comparison</caption>
<columns>
<column>Library</column>
<column>Pros</column>
</columns>
<row>
<cell>oauth2</cell>
<cell>Official</cell>
</row>
</table>
</content>
<depends-on step="1"/>
</step>

<step number="3" type="action" priority="high">
<title>Configure test environment</title>
<content>
<list type="ordered">
<item>Create Google Cloud project</item>
<item>Create GitHub OAuth App</item>
</list>
</content>
<depends-on step="1"/>
</step>
</ralph-implementation-steps>

<ralph-critical-files>
<primary-files>
<file path="src/config/oauth.rs" action="create" estimated-changes="~50 lines"/>
<file path="src/auth/oauth2.rs" action="create" estimated-changes="~200 lines"/>
</primary-files>
<reference-files>
<file path="src/auth/mod.rs" purpose="Existing auth patterns"/>
</reference-files>
</ralph-critical-files>

<ralph-risks-mitigations>
<risk-pair severity="high">
<risk>Token interception</risk>
<mitigation>Use HTTPS, implement PKCE</mitigation>
</risk-pair>
<risk-pair severity="medium">
<risk>Provider API changes</risk>
<mitigation>Abstract behind interfaces</mitigation>
</risk-pair>
</ralph-risks-mitigations>

<ralph-verification-strategy>
<verification>
<method>Run integration tests</method>
<expected-outcome>OAuth flows complete successfully</expected-outcome>
</verification>
<verification>
<method>Manual testing</method>
<expected-outcome>Users can sign in</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let plan = result.unwrap();

        // Summary checks
        assert_eq!(plan.summary.scope_items.len(), 4);
        assert_eq!(plan.summary.scope_items[0].count, Some("3".to_string()));
        assert_eq!(
            plan.summary.scope_items[0].category,
            Some("auth".to_string())
        );

        // Steps checks
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].number, 1);
        assert_eq!(plan.steps[0].step_type, StepType::FileChange);
        assert_eq!(plan.steps[0].target_files.len(), 2);

        assert_eq!(plan.steps[1].number, 2);
        assert_eq!(plan.steps[1].step_type, StepType::Research);
        assert_eq!(plan.steps[1].depends_on, vec![1]);

        assert_eq!(plan.steps[2].number, 3);
        assert_eq!(plan.steps[2].step_type, StepType::Action);
        assert_eq!(plan.steps[2].depends_on, vec![1]);

        // Critical files checks
        assert_eq!(plan.critical_files.primary_files.len(), 2);
        assert_eq!(plan.critical_files.reference_files.len(), 1);

        // Risks checks
        assert_eq!(plan.risks_mitigations.len(), 2);
        assert_eq!(plan.risks_mitigations[0].severity, Some(Severity::High));

        // Verification checks
        assert_eq!(plan.verification_strategy.len(), 2);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MISSING REQUIRED SECTIONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_missing_implementation_steps() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
        assert!(err.element_path.contains("ralph-implementation-steps"));
        // Verify error message is helpful for reprompting
        let retry_msg = err.format_for_ai_retry();
        assert!(retry_msg.contains("MISSING REQUIRED ELEMENT"));
        assert!(retry_msg.contains("ralph-implementation-steps"));
    }

    #[test]
    fn test_missing_critical_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
        assert!(err.element_path.contains("ralph-critical-files"));
    }

    #[test]
    fn test_missing_risks_mitigations() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
        assert!(err.element_path.contains("ralph-risks-mitigations"));
    }

    #[test]
    fn test_missing_verification_strategy() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
        assert!(err.element_path.contains("ralph-verification-strategy"));
    }

    #[test]
    fn test_empty_implementation_steps() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("ralph-implementation-steps"));
        assert!(err.suggestion.contains("step"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // STEP VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_step_missing_number_attribute() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("step"));
        assert!(err.expected.contains("number"));
        let retry_msg = err.format_for_ai_retry();
        assert!(retry_msg.contains("number"));
    }

    #[test]
    fn test_step_invalid_number_attribute() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="abc" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::InvalidContent);
        assert!(err.element_path.contains("number"));
        assert!(err.found.contains("abc"));
    }

    #[test]
    fn test_step_missing_title() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("title"));
        let retry_msg = err.format_for_ai_retry();
        assert!(retry_msg.contains("title"));
    }

    #[test]
    fn test_step_missing_content() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test step</title>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("content"));
        let retry_msg = err.format_for_ai_retry();
        assert!(retry_msg.contains("content"));
        assert!(retry_msg.contains("paragraph"));
    }

    #[test]
    fn test_step_type_defaults_to_file_change() {
        // When no type is specified, default is file-change which requires target-files
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1">
<title>Test</title>
<target-files>
<file path="test.rs" action="modify"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        // Default type should be FileChange
        assert_eq!(plan.steps[0].step_type, StepType::FileChange);
    }

    #[test]
    fn test_step_without_type_requires_target_files() {
        // When no type is specified, default is file-change which requires target-files
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should error because default type (file-change) requires target-files
        assert!(err.element_path.contains("target-files"));
    }

    #[test]
    fn test_step_all_types_valid() {
        for (type_str, expected_type) in [
            ("file-change", StepType::FileChange),
            ("action", StepType::Action),
            ("research", StepType::Research),
        ] {
            let xml = format!(
                r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="{}">
<title>Test</title>
{}
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#,
                type_str,
                if type_str == "file-change" {
                    r#"<target-files><file path="test.rs" action="modify"/></target-files>"#
                } else {
                    ""
                }
            );

            let result = validate_plan_xml(&xml);
            assert!(
                result.is_ok(),
                "Failed for type '{}': {:?}",
                type_str,
                result.err()
            );
            assert_eq!(result.unwrap().steps[0].step_type, expected_type);
        }
    }

    #[test]
    fn test_step_all_priorities_valid() {
        for (priority_str, expected_priority) in [
            ("high", Priority::High),
            ("medium", Priority::Medium),
            ("low", Priority::Low),
        ] {
            let xml = format!(
                r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action" priority="{}">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#,
                priority_str
            );

            let result = validate_plan_xml(&xml);
            assert!(
                result.is_ok(),
                "Failed for priority '{}': {:?}",
                priority_str,
                result.err()
            );
            assert_eq!(result.unwrap().steps[0].priority, Some(expected_priority));
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FILE ELEMENT VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_target_file_missing_path() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<target-files>
<file action="modify"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("file"));
        assert!(err.expected.contains("path"));
    }

    #[test]
    fn test_target_file_missing_action() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<target-files>
<file path="src/test.rs"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("file"));
        assert!(err.expected.contains("action"));
    }

    #[test]
    fn test_target_file_invalid_action() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<target-files>
<file path="src/test.rs" action="update"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::InvalidContent);
        assert!(err.found.contains("update"));
        assert!(
            err.suggestion.contains("create")
                || err.suggestion.contains("modify")
                || err.suggestion.contains("delete")
        );
    }

    #[test]
    fn test_target_file_all_actions_valid() {
        for action in ["create", "modify", "delete"] {
            let xml = format!(
                r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<target-files>
<file path="src/test.rs" action="{}"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#,
                action
            );

            let result = validate_plan_xml(&xml);
            assert!(
                result.is_ok(),
                "Failed for action '{}': {:?}",
                action,
                result.err()
            );
        }
    }

    #[test]
    fn test_multiple_target_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<target-files>
<file path="src/a.rs" action="create"/>
<file path="src/b.rs" action="modify"/>
<file path="src/c.rs" action="delete"/>
</target-files>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.steps[0].target_files.len(), 3);
        assert_eq!(plan.steps[0].target_files[0].action, FileAction::Create);
        assert_eq!(plan.steps[0].target_files[1].action, FileAction::Modify);
        assert_eq!(plan.steps[0].target_files[2].action, FileAction::Delete);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RICH CONTENT PARSING TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_content_element_is_rejected() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        // Empty content should be rejected - a step must have meaningful content
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("content"));
        // Error message should suggest what to add
        let retry_msg = err.format_for_ai_retry();
        assert!(retry_msg.contains("paragraph") || retry_msg.contains("code-block"));
    }

    #[test]
    fn test_mixed_content_elements() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
<paragraph>First paragraph</paragraph>
<code-block language="rust">let x = 1;</code-block>
<list type="unordered">
<item>Item A</item>
<item>Item B</item>
</list>
<paragraph>Final paragraph</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.steps[0].content.elements.len(), 4);

        // Check types in order
        assert!(matches!(
            plan.steps[0].content.elements[0],
            ContentElement::Paragraph(_)
        ));
        assert!(matches!(
            plan.steps[0].content.elements[1],
            ContentElement::CodeBlock(_)
        ));
        assert!(matches!(
            plan.steps[0].content.elements[2],
            ContentElement::List(_)
        ));
        assert!(matches!(
            plan.steps[0].content.elements[3],
            ContentElement::Paragraph(_)
        ));
    }

    #[test]
    fn test_code_block_without_attributes() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
<code-block>plain code here</code-block>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[0] {
            assert!(cb.language.is_none());
            assert!(cb.filename.is_none());
            assert!(cb.content.contains("plain code"));
        } else {
            panic!("Expected code block");
        }
    }

    #[test]
    fn test_list_unordered() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
<list type="unordered">
<item>Bullet 1</item>
<item>Bullet 2</item>
</list>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
            assert_eq!(l.list_type, ListType::Unordered);
            assert_eq!(l.items.len(), 2);
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_heading_element() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
<heading level="2">Section Header</heading>
<paragraph>Content under the heading</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        if let ContentElement::Heading(h) = &plan.steps[0].content.elements[0] {
            assert_eq!(h.level, 2);
            assert_eq!(h.text, "Section Header");
        } else {
            panic!("Expected heading");
        }
    }

    #[test]
    fn test_table_without_caption() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content>
<table>
<columns>
<column>A</column>
<column>B</column>
</columns>
<row>
<cell>1</cell>
<cell>2</cell>
</row>
</table>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        if let ContentElement::Table(t) = &plan.steps[0].content.elements[0] {
            assert!(t.caption.is_none());
            assert_eq!(t.columns.len(), 2);
            assert_eq!(t.rows.len(), 1);
        } else {
            panic!("Expected table");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CRITICAL FILES SECTION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_critical_files_missing_primary_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<reference-files>
<file path="ref.rs" purpose="reference"/>
</reference-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("primary-files"));
    }

    #[test]
    fn test_critical_files_empty_primary_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files>
</primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("primary-files"));
        assert!(err.suggestion.contains("file"));
    }

    #[test]
    fn test_critical_files_with_reference_files() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files>
<file path="main.rs" action="modify" estimated-changes="~50 lines"/>
</primary-files>
<reference-files>
<file path="lib.rs" purpose="Existing patterns"/>
<file path="utils.rs" purpose="Helper functions"/>
</reference-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.critical_files.primary_files.len(), 1);
        assert_eq!(plan.critical_files.reference_files.len(), 2);
        assert_eq!(
            plan.critical_files.primary_files[0].estimated_changes,
            Some("~50 lines".to_string())
        );
        assert_eq!(
            plan.critical_files.reference_files[0].purpose,
            "Existing patterns"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RISKS AND MITIGATIONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_risk_pair_missing_risk() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair>
<mitigation>M</mitigation>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("risk"));
    }

    #[test]
    fn test_risk_pair_missing_mitigation() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair>
<risk>R</risk>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("mitigation"));
    }

    #[test]
    fn test_risk_pair_all_severities() {
        for severity in ["high", "medium", "low"] {
            let xml = format!(
                r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair severity="{}">
<risk>R</risk>
<mitigation>M</mitigation>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#,
                severity
            );

            let result = validate_plan_xml(&xml);
            assert!(
                result.is_ok(),
                "Failed for severity '{}': {:?}",
                severity,
                result.err()
            );
        }
    }

    #[test]
    fn test_empty_risks_mitigations() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("ralph-risks-mitigations"));
        assert!(err.suggestion.contains("risk-pair"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // VERIFICATION STRATEGY TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_verification_missing_method() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<expected-outcome>O</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("method"));
    }

    #[test]
    fn test_verification_missing_expected_outcome() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<method>M</method>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("expected-outcome"));
    }

    #[test]
    fn test_empty_verification_strategy() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("ralph-verification-strategy"));
        assert!(err.suggestion.contains("verification"));
    }

    #[test]
    fn test_multiple_verifications() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<method>Unit tests</method>
<expected-outcome>All pass</expected-outcome>
</verification>
<verification>
<method>Integration tests</method>
<expected-outcome>All pass</expected-outcome>
</verification>
<verification>
<method>Manual review</method>
<expected-outcome>Approved</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.verification_strategy.len(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EDGE CASES AND MALFORMED XML TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_xml() {
        let result = validate_plan_xml("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
    }

    #[test]
    fn test_whitespace_only_xml() {
        let result = validate_plan_xml("   \n\t  \n  ");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, XsdErrorType::MissingRequiredElement);
    }

    #[test]
    fn test_unclosed_tag() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</ralph-implementation-steps>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_xml_with_preamble_text() {
        let xml = r#"Here is the plan:

<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        // The validator should handle preamble text gracefully
        let result = validate_plan_xml(xml);
        // This may fail or succeed depending on implementation - we just want no panic
        // If it fails, the error should be meaningful
        if let Err(err) = &result {
            assert!(!err.suggestion.is_empty());
        }
    }

    #[test]
    fn test_missing_context_in_summary() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("context"));
    }

    #[test]
    fn test_missing_scope_items_in_summary() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.element_path.contains("scope-items"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ERROR MESSAGE QUALITY TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_error_message_includes_element_path() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
</scope-items>
</ralph-summary>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let retry_msg = err.format_for_ai_retry();

        // Error message should be specific and actionable
        assert!(retry_msg.contains("scope-items"));
        assert!(retry_msg.contains("How to fix"));
    }

    #[test]
    fn test_error_message_includes_what_was_found() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>one</scope-item>
<scope-item>two</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();

        // Should tell us what was found (2 items)
        assert!(err.found.contains("2"));
        // Should tell us what was expected (3 minimum)
        assert!(err.expected.contains("3"));
    }

    #[test]
    fn test_error_message_provides_actionable_suggestion() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change">
<title>Test</title>
<content><paragraph>Test</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err());
        let err = result.unwrap_err();

        // Suggestion should show how to add target-files
        assert!(err.suggestion.contains("target-files"));
        assert!(err.suggestion.contains("file"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // DEPENDS-ON TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_step_with_multiple_dependencies() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Step 1</title>
<content><paragraph>First</paragraph></content>
</step>
<step number="2" type="action">
<title>Step 2</title>
<content><paragraph>Second</paragraph></content>
</step>
<step number="3" type="action">
<title>Step 3</title>
<content><paragraph>Third - depends on 1 and 2</paragraph></content>
<depends-on step="1"/>
<depends-on step="2"/>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(plan.steps[2].depends_on, vec![1, 2]);
    }

    #[test]
    fn test_step_optional_fields() {
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change" priority="high">
<title>Complete step</title>
<target-files>
<file path="src/main.rs" action="modify"/>
</target-files>
<location>After the imports section</location>
<rationale>This change is needed because...</rationale>
<content><paragraph>Detailed description</paragraph></content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.rs" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Error: {:?}", result.err());
        let plan = result.unwrap();
        assert_eq!(
            plan.steps[0].location,
            Some("After the imports section".to_string())
        );
        assert_eq!(
            plan.steps[0].rationale,
            Some("This change is needed because...".to_string())
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // COMPREHENSIVE REAL-WORLD PLAN TEST
    // ═══════════════════════════════════════════════════════════════════════════

    /// Test that validates a comprehensive real-world plan XML file
    /// This tests all features: multiple steps, dependencies, rich content, etc.
    /// Based on the 15-step documentation enhancement plan example.
    #[test]
    fn test_comprehensive_real_world_plan() {
        let xml = include_str!("test_data/example_plan.xml");

        let result = validate_plan_xml(xml);
        assert!(result.is_ok(), "Validation failed: {:?}", result.err());

        let plan = result.unwrap();

        // ═══════════════════════════════════════════════════════════════════════
        // SUMMARY VALIDATION
        // ═══════════════════════════════════════════════════════════════════════

        // Context should be present and non-empty
        assert!(
            !plan.summary.context.is_empty(),
            "Context should not be empty"
        );
        assert!(
            plan.summary
                .context
                .contains("website design specification"),
            "Context should mention the main topic"
        );

        // Should have exactly 8 scope items (15 sections, 50 flags, 18 agents, etc.)
        assert_eq!(plan.summary.scope_items.len(), 8, "Expected 8 scope items");

        // Verify scope items have counts and categories
        let first_scope = &plan.summary.scope_items[0];
        assert_eq!(first_scope.count, Some("15".to_string()));
        assert_eq!(first_scope.category, Some("sections".to_string()));
        assert!(first_scope.description.contains("documentation"));

        // Check various scope items
        let flags_scope = plan
            .summary
            .scope_items
            .iter()
            .find(|s| s.category == Some("flags".to_string()));
        assert!(flags_scope.is_some(), "Should have flags scope item");
        assert_eq!(flags_scope.unwrap().count, Some("50".to_string()));

        let agents_scope = plan
            .summary
            .scope_items
            .iter()
            .find(|s| s.category == Some("agents".to_string()));
        assert!(agents_scope.is_some(), "Should have agents scope item");
        assert_eq!(agents_scope.unwrap().count, Some("18".to_string()));

        let providers_scope = plan
            .summary
            .scope_items
            .iter()
            .find(|s| s.category == Some("providers".to_string()));
        assert!(
            providers_scope.is_some(),
            "Should have providers scope item"
        );
        assert_eq!(providers_scope.unwrap().count, Some("45".to_string()));

        // ═══════════════════════════════════════════════════════════════════════
        // STEPS VALIDATION - All 15 steps
        // ═══════════════════════════════════════════════════════════════════════

        // Should have 15 steps
        assert_eq!(plan.steps.len(), 15, "Expected 15 implementation steps");

        // Step 1: CLI reference - file-change with high priority
        let step1 = &plan.steps[0];
        assert_eq!(step1.number, 1);
        assert_eq!(step1.step_type, StepType::FileChange);
        assert_eq!(step1.priority, Some(Priority::High));
        assert!(step1.title.contains("CLI reference"));
        assert_eq!(step1.target_files.len(), 1);
        assert_eq!(step1.target_files[0].path, "docs/website-design-spec.md");
        assert_eq!(step1.target_files[0].action, FileAction::Modify);
        assert!(step1.location.is_some());
        assert!(step1.rationale.is_some());
        assert!(step1.depends_on.is_empty());

        // Verify step 1 has rich content with table, headings, and lists
        assert!(!step1.content.elements.is_empty());
        let has_table = step1
            .content
            .elements
            .iter()
            .any(|e| matches!(e, ContentElement::Table(_)));
        assert!(has_table, "Step 1 should have a table");

        let has_list = step1
            .content
            .elements
            .iter()
            .any(|e| matches!(e, ContentElement::List(_)));
        assert!(has_list, "Step 1 should have a list");

        let has_heading = step1
            .content
            .elements
            .iter()
            .any(|e| matches!(e, ContentElement::Heading(_)));
        assert!(has_heading, "Step 1 should have headings");

        // Step 2: Configuration schema - depends on step 1, has code blocks
        let step2 = &plan.steps[1];
        assert_eq!(step2.number, 2);
        assert_eq!(step2.step_type, StepType::FileChange);
        assert_eq!(step2.priority, Some(Priority::High));
        assert_eq!(step2.depends_on, vec![1]);

        // Verify step 2 has multiple code blocks
        let code_blocks: Vec<_> = step2
            .content
            .elements
            .iter()
            .filter(|e| matches!(e, ContentElement::CodeBlock(_)))
            .collect();
        assert!(
            code_blocks.len() >= 3,
            "Step 2 should have multiple code blocks"
        );

        // Check first code block details
        if let Some(ContentElement::CodeBlock(cb)) = step2
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::CodeBlock(_)))
        {
            assert_eq!(cb.language, Some("toml".to_string()));
            assert_eq!(cb.filename, Some("ralph-workflow.toml".to_string()));
            assert!(cb.content.contains("[general]"));
        }

        // Step 3: Built-in agents - high priority
        let step3 = &plan.steps[2];
        assert_eq!(step3.number, 3);
        assert!(step3.title.contains("agents"));
        assert_eq!(step3.depends_on, vec![2]);

        // Step 4: OpenCode providers - medium priority
        let step4 = &plan.steps[3];
        assert_eq!(step4.number, 4);
        assert_eq!(step4.priority, Some(Priority::Medium));
        assert!(step4.title.contains("OpenCode"));

        // Verify step 4 has a large table with provider categories
        if let Some(ContentElement::Table(t)) = step4
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::Table(_)))
        {
            assert!(t.rows.len() >= 10, "Provider table should have many rows");
        }

        // Step 5: Workflow pipeline - high priority
        let step5 = &plan.steps[4];
        assert_eq!(step5.number, 5);
        assert_eq!(step5.priority, Some(Priority::High));
        assert!(step5.title.contains("workflow"));

        // Step 6: Checkpoint system - medium priority
        let step6 = &plan.steps[5];
        assert_eq!(step6.number, 6);
        assert_eq!(step6.priority, Some(Priority::Medium));
        assert!(step6.title.contains("checkpoint"));
        assert_eq!(step6.depends_on, vec![5]);

        // Step 7: Prompt templates
        let step7 = &plan.steps[6];
        assert_eq!(step7.number, 7);
        assert!(step7.title.contains("template"));

        // Verify step 7 has a table with template variables
        let has_table = step7
            .content
            .elements
            .iter()
            .any(|e| matches!(e, ContentElement::Table(_)));
        assert!(has_table, "Step 7 should have a variables table");

        // Step 8: Language detection
        let step8 = &plan.steps[7];
        assert_eq!(step8.number, 8);
        assert!(step8.title.contains("language"));

        // Step 9: JSON parser system
        let step9 = &plan.steps[8];
        assert_eq!(step9.number, 9);
        assert!(step9.title.contains("JSON parser") || step9.title.contains("parser"));

        // Verify step 9 has parser types table
        if let Some(ContentElement::Table(t)) = step9
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::Table(_)))
        {
            assert_eq!(
                t.rows.len(),
                5,
                "Parser table should have 5 rows (5 parsers)"
            );
        }

        // Step 10: CCS integration
        let step10 = &plan.steps[9];
        assert_eq!(step10.number, 10);
        assert!(step10.title.contains("CCS"));

        // Step 11: Error handling and fallback
        let step11 = &plan.steps[10];
        assert_eq!(step11.number, 11);
        assert!(step11.title.contains("error") || step11.title.contains("fallback"));

        // Step 12: Work guide templates - low priority
        let step12 = &plan.steps[11];
        assert_eq!(step12.number, 12);
        assert_eq!(step12.priority, Some(Priority::Low));
        assert!(step12.title.contains("work guide"));

        // Verify step 12 has list of 20 work guides
        if let Some(ContentElement::List(l)) = step12
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::List(_)))
        {
            assert_eq!(l.items.len(), 20, "Should have 20 work guides");
        }

        // Step 13: Troubleshooting matrix - low priority
        let step13 = &plan.steps[12];
        assert_eq!(step13.number, 13);
        assert_eq!(step13.priority, Some(Priority::Low));
        assert!(step13.title.contains("troubleshooting"));

        // Verify step 13 has troubleshooting table
        if let Some(ContentElement::Table(t)) = step13
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::Table(_)))
        {
            assert!(
                t.rows.len() >= 10,
                "Troubleshooting table should have many rows"
            );
        }

        // Step 14: Git integration - low priority
        let step14 = &plan.steps[13];
        assert_eq!(step14.number, 14);
        assert_eq!(step14.priority, Some(Priority::Low));
        assert!(step14.title.contains("git"));

        // Step 15: .agent/ directory structure - low priority
        let step15 = &plan.steps[14];
        assert_eq!(step15.number, 15);
        assert_eq!(step15.priority, Some(Priority::Low));
        assert!(step15.title.contains(".agent/") || step15.title.contains("directory"));
        assert_eq!(step15.depends_on, vec![14]);

        // Verify step 15 has a code block with directory tree
        if let Some(ContentElement::CodeBlock(cb)) = step15
            .content
            .elements
            .iter()
            .find(|e| matches!(e, ContentElement::CodeBlock(_)))
        {
            assert!(cb.content.contains(".agent/"));
            assert!(cb.content.contains("checkpoint.json"));
            assert!(cb.content.contains("logs/"));
        }

        // ═══════════════════════════════════════════════════════════════════════
        // VERIFY DEPENDENCY CHAIN
        // ═══════════════════════════════════════════════════════════════════════

        // Steps should form a proper dependency chain
        assert!(
            plan.steps[0].depends_on.is_empty(),
            "Step 1 has no dependencies"
        );
        for i in 1..15 {
            assert_eq!(
                plan.steps[i].depends_on,
                vec![i as u32],
                "Step {} should depend on step {}",
                i + 1,
                i
            );
        }

        // ═══════════════════════════════════════════════════════════════════════
        // CRITICAL FILES VALIDATION
        // ═══════════════════════════════════════════════════════════════════════

        // Should have 1 primary file
        assert_eq!(
            plan.critical_files.primary_files.len(),
            1,
            "Expected 1 primary file"
        );
        let primary = &plan.critical_files.primary_files[0];
        assert_eq!(primary.path, "docs/website-design-spec.md");
        assert_eq!(primary.action, FileAction::Modify);
        assert!(primary.estimated_changes.is_some());
        assert!(primary.estimated_changes.as_ref().unwrap().contains("3000"));

        // Should have 13 reference files
        assert_eq!(
            plan.critical_files.reference_files.len(),
            13,
            "Expected 13 reference files"
        );

        // Check reference files have purposes
        for ref_file in &plan.critical_files.reference_files {
            assert!(
                !ref_file.path.is_empty(),
                "Reference file path should not be empty"
            );
            assert!(
                !ref_file.purpose.is_empty(),
                "Reference file purpose should not be empty"
            );
        }

        // Verify specific reference files
        let has_cli_ref = plan
            .critical_files
            .reference_files
            .iter()
            .any(|f| f.path.contains("args.rs"));
        assert!(has_cli_ref, "Should reference CLI args source");

        let has_config_ref = plan
            .critical_files
            .reference_files
            .iter()
            .any(|f| f.path.contains("unified.rs"));
        assert!(has_config_ref, "Should reference config source");

        // ═══════════════════════════════════════════════════════════════════════
        // RISKS AND MITIGATIONS VALIDATION
        // ═══════════════════════════════════════════════════════════════════════

        // Should have 5 risk pairs
        assert_eq!(
            plan.risks_mitigations.len(),
            5,
            "Expected 5 risk-mitigation pairs"
        );

        // Check severities - should be high, medium, low, low, medium
        assert_eq!(plan.risks_mitigations[0].severity, Some(Severity::High));
        assert_eq!(plan.risks_mitigations[1].severity, Some(Severity::Medium));
        assert_eq!(plan.risks_mitigations[2].severity, Some(Severity::Low));
        assert_eq!(plan.risks_mitigations[3].severity, Some(Severity::Low));
        assert_eq!(plan.risks_mitigations[4].severity, Some(Severity::Medium));

        // Check content of first risk
        assert!(plan.risks_mitigations[0].risk.contains("outdated"));
        assert!(plan.risks_mitigations[0]
            .mitigation
            .contains("Reference source file"));

        // ═══════════════════════════════════════════════════════════════════════
        // VERIFICATION STRATEGY VALIDATION
        // ═══════════════════════════════════════════════════════════════════════

        // Should have 7 verification items
        assert_eq!(
            plan.verification_strategy.len(),
            7,
            "Expected 7 verification items"
        );

        // Check each verification has method and expected outcome
        for verification in &plan.verification_strategy {
            assert!(
                !verification.method.is_empty(),
                "Verification method should not be empty"
            );
            assert!(
                !verification.expected_outcome.is_empty(),
                "Expected outcome should not be empty"
            );
        }

        // Check specific verifications
        assert!(plan.verification_strategy[0]
            .method
            .contains("Cross-reference"));
        assert!(plan.verification_strategy[1].method.contains("CLI flags"));
        assert!(plan.verification_strategy[2].method.contains("config"));
        assert!(plan.verification_strategy[3].method.contains("agent"));
        assert!(plan.verification_strategy[4].method.contains("template"));
        assert!(plan.verification_strategy[5].method.contains("provider"));
        assert!(
            plan.verification_strategy[6]
                .method
                .contains("existing docs")
                || plan.verification_strategy[6].method.contains("Review")
        );
    }

    /// Test that the error message for invalid XML is actionable for AI retry
    #[test]
    fn test_real_world_plan_error_messages_are_actionable() {
        // Create a modified version with a missing required element
        let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test comprehensive plan</context>
<scope-items>
<scope-item count="15" category="sections">major sections</scope-item>
<scope-item count="50" category="flags">CLI flags</scope-item>
<scope-item count="18" category="agents">built-in agents</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change" priority="high">
<title>Add CLI reference</title>
<!-- Missing target-files for file-change step! -->
<content>
<paragraph>Add comprehensive CLI documentation.</paragraph>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files>
<file path="docs/spec.md" action="modify" estimated-changes="~3000 lines"/>
</primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair severity="high">
<risk>Documentation outdated</risk>
<mitigation>Reference source files</mitigation>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<method>Cross-reference with code</method>
<expected-outcome>All claims traceable</expected-outcome>
</verification>
</ralph-verification-strategy>
</ralph-plan>"#;

        let result = validate_plan_xml(xml);
        assert!(result.is_err(), "Should fail due to missing target-files");

        let err = result.unwrap_err();

        // Error should identify the problem element
        assert!(
            err.element_path.contains("target-files"),
            "Error path should mention target-files"
        );

        // Error message should be actionable
        let retry_msg = err.format_for_ai_retry();
        assert!(
            retry_msg.contains("MISSING REQUIRED ELEMENT"),
            "Should indicate missing element"
        );
        assert!(
            retry_msg.contains("target-files"),
            "Should mention target-files"
        );
        assert!(
            retry_msg.contains("How to fix"),
            "Should provide fix guidance"
        );
        assert!(retry_msg.contains("<file"), "Should show example fix");
    }
}
