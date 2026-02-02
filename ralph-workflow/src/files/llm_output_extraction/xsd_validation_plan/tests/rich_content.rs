//! Tests for rich content parsing elements.
//!
//! This module tests the parsing of rich content elements like paragraphs,
//! code blocks, lists, tables, and headings within plan step content.

use super::*;

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
