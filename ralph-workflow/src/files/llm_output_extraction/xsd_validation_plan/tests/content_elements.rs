//! Tests for rich content parsing.
//!
//! These tests verify that the parser correctly handles rich content elements
//! like paragraphs, code blocks, lists, tables, and headings. Also includes
//! tests for list item flexibility, CDATA support, and realistic LLM output.

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

// ═══════════════════════════════════════════════════════════════════════════
// LIST ITEM FLEXIBILITY TESTS - Block elements inside list items
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_list_item_with_code_block() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test code blocks in list items</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Steps with code examples</title>
<content>
<list type="ordered">
<item>First, create the config:
<code-block language="rust">
struct Config {
name: String,
}
</code-block>
</item>
<item>Then use it in main</item>
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
    assert!(
        result.is_ok(),
        "List items with code-block should be valid: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
        assert_eq!(l.items.len(), 2);
        // First item should have text content (code-block is stripped for inline parsing)
        assert!(!l.items[0].content.is_empty());
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_list_item_with_paragraph() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test paragraphs in list items</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Steps with paragraphs</title>
<content>
<list type="unordered">
<item>
<paragraph>This is a detailed explanation of the first step.</paragraph>
</item>
<item>Simple second item</item>
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
    assert!(
        result.is_ok(),
        "List items with paragraph should be valid: {:?}",
        result.err()
    );
}

#[test]
fn test_list_item_with_inline_code() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test inline code in list items</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Steps with inline code</title>
<content>
<list type="ordered">
<item>Run <code>cargo build</code> to compile</item>
<item>Execute <code>./target/debug/app</code></item>
<item>Check the <emphasis>output</emphasis> carefully</item>
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
    assert!(
        result.is_ok(),
        "List items with inline code should be valid: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
        assert_eq!(l.items.len(), 3);
        // Check that inline code was parsed
        let first_item = &l.items[0].content;
        assert!(
            first_item
                .iter()
                .any(|e| matches!(e, InlineElement::Code(_))),
            "Should contain inline code element"
        );
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_list_item_with_mixed_block_and_inline() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test mixed content in list items</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Complex list items</title>
<content>
<list type="ordered">
<item>First, understand the <code>Config</code> struct:
<code-block language="rust">
struct Config {
    name: String,
}
</code-block>
Then implement the <emphasis>required</emphasis> trait.
</item>
<item>Next step with <code>simple</code> code</item>
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
    assert!(
        result.is_ok(),
        "List items with mixed block and inline should be valid: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
        assert_eq!(l.items.len(), 2);
        // First item should have inline elements
        assert!(!l.items[0].content.is_empty());
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_list_item_with_nested_list_and_code_block() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test nested structures in list items</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Nested list test</title>
<content>
<list type="ordered">
<item>Top-level item with nested list:
<list type="unordered">
<item>Nested item 1</item>
<item>Nested item 2</item>
</list>
</item>
<item>Another top-level item with code:
<code-block language="rust">
let x = 42;
</code-block>
</item>
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
    assert!(
        result.is_ok(),
        "List items with nested list and code-block should be valid: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
        assert_eq!(l.items.len(), 2);
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_strip_block_elements_helper() {
    // Test the helper function directly
    let content = r#"First text <code-block language="rust">fn main() {}</code-block> middle <paragraph>para</paragraph> end"#;
    let stripped = crate::files::llm_output_extraction::xsd_validation_plan::strip_block_elements_for_inline_parsing(content);
    assert!(
        !stripped.contains("<code-block"),
        "Should strip code-block tags"
    );
    assert!(
        !stripped.contains("<paragraph"),
        "Should strip paragraph tags"
    );
    assert!(stripped.contains("First text"), "Should preserve text");
    assert!(stripped.contains("middle"), "Should preserve middle text");
    assert!(stripped.contains("end"), "Should preserve end text");
}

#[test]
fn test_strip_block_elements_with_nested_list() {
    let content = r#"Text before <list type="ordered"><item>nested</item></list> text after"#;
    let stripped = crate::files::llm_output_extraction::xsd_validation_plan::strip_block_elements_for_inline_parsing(content);
    assert!(!stripped.contains("<list"), "Should strip list tags");
    assert!(stripped.contains("Text before"), "Should preserve text");
    assert!(stripped.contains("text after"), "Should preserve text");
}

// ─────────────────────────────────────────────────────────────────────────────
// CDATA SUPPORT TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_code_block_with_cdata() {
    // CDATA allows including <, >, & without escaping
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test CDATA support</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>CDATA Test</title>
<content>
<code-block language="rust"><![CDATA[
fn compare<T: Ord>(a: T, b: T) -> bool {
    a < b && b > a
}
]]></code-block>
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
    assert!(
        result.is_ok(),
        "CDATA should be parsed correctly: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[0] {
        // CDATA content should be preserved with actual < and > characters
        assert!(cb.content.contains("a < b"), "Should contain unescaped <");
        assert!(cb.content.contains("b > a"), "Should contain unescaped >");
        assert!(cb.content.contains("T: Ord"), "Should contain generic");
    } else {
        panic!("Expected code block");
    }
}

#[test]
fn test_code_block_with_cdata_complex_code() {
    // More complex code with various special characters
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Complex CDATA test</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Complex Code</title>
<content>
<code-block language="typescript"><![CDATA[
type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };

function process<T>(items: T[]): T[] {
    return items.filter(x => x !== null && x !== undefined);
}

const html = "<div class='container'><span>Hello</span></div>";
const condition = a < b && c > d || e <= f && g >= h;
]]></code-block>
</content>
</step>
</ralph-implementation-steps>
<ralph-critical-files>
<primary-files><file path="test.ts" action="create"/></primary-files>
</ralph-critical-files>
<ralph-risks-mitigations>
<risk-pair><risk>R</risk><mitigation>M</mitigation></risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification><method>M</method><expected-outcome>O</expected-outcome></verification>
</ralph-verification-strategy>
</ralph-plan>"#;

    let result = validate_plan_xml(xml);
    assert!(
        result.is_ok(),
        "Complex CDATA should parse: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[0] {
        assert!(cb.content.contains("Result<T, E>"), "Should have generics");
        assert!(cb.content.contains("<div"), "Should have HTML");
        assert!(
            cb.content.contains("a < b && c > d"),
            "Should have comparisons"
        );
        assert!(cb.content.contains("=>"), "Should have arrow function");
    } else {
        panic!("Expected code block");
    }
}

// =========================================================================
// REALISTIC LLM OUTPUT TESTS FOR PLANS
// These test actual patterns that LLMs produce when following the prompts
// =========================================================================

/// Helper to create a minimal valid plan wrapper around step content
fn wrap_in_plan(step_content: &str) -> String {
    format!(
        r#"<ralph-plan>
<ralph-summary>
<context>Test context</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Test Step</title>
<content>
{step_content}
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
</ralph-plan>"#
    )
}

#[test]
fn test_llm_plan_with_rust_generics_in_cdata() {
    // LLM correctly uses CDATA for Rust code with generics
    let xml = wrap_in_plan(
        r#"<paragraph>Implement the generic parser:</paragraph>
<code-block language="rust"><![CDATA[
pub fn parse<T: FromStr>(input: &str) -> Result<T, ParseError> {
    input.parse().map_err(|_| ParseError::Invalid)
}
]]></code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with CDATA generics should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[1] {
        assert!(cb.content.contains("T: FromStr"));
        assert!(cb.content.contains("Result<T, ParseError>"));
    }
}

#[test]
fn test_llm_plan_with_comparison_operators_in_cdata() {
    // LLM correctly uses CDATA for code with comparisons
    let xml = wrap_in_plan(
        r#"<paragraph>Add boundary validation:</paragraph>
<code-block language="rust"><![CDATA[
fn validate_range(value: i32, min: i32, max: i32) -> bool {
    value >= min && value <= max && min < max
}
]]></code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with CDATA comparisons should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[1] {
        assert!(cb.content.contains("value >= min"));
        assert!(cb.content.contains("min < max"));
    }
}

#[test]
fn test_llm_plan_with_html_template_in_cdata() {
    // LLM correctly uses CDATA for HTML templates
    let xml = wrap_in_plan(
        r#"<paragraph>Create the HTML template:</paragraph>
<code-block language="html"><![CDATA[
<div class="container">
    <span v-if="count > 0">{{ count }} items</span>
    <button @click="count < 10 && increment()">Add</button>
</div>
]]></code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with CDATA HTML should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[1] {
        assert!(cb.content.contains("<div class=\"container\">"));
        assert!(cb.content.contains("count > 0"));
        assert!(cb.content.contains("count < 10"));
    }
}

#[test]
fn test_llm_plan_with_inline_code_escaped() {
    // LLM correctly escapes inline code references
    let xml = wrap_in_plan(
        r#"<paragraph>The function <code>compare&lt;T&gt;</code> should return <code>Option&lt;Ordering&gt;</code> when <code>a &lt; b</code>.</paragraph>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with escaped inline code should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::Paragraph(p) = &plan.steps[0].content.elements[0] {
        let text: String = p
            .content
            .iter()
            .map(|e| match e {
                crate::files::llm_output_extraction::xsd_validation_plan::InlineElement::Text(
                    t,
                ) => t.clone(),
                crate::files::llm_output_extraction::xsd_validation_plan::InlineElement::Code(
                    c,
                ) => c.clone(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("compare<T>"));
        assert!(text.contains("Option<Ordering>"));
        assert!(text.contains("a < b"));
    }
}

#[test]
fn test_llm_plan_with_typescript_arrow_functions_in_cdata() {
    // LLM correctly uses CDATA for TypeScript with arrow functions and generics
    let xml = wrap_in_plan(
        r#"<paragraph>Implement the filter utility:</paragraph>
<code-block language="typescript"><![CDATA[
const filter = <T>(items: T[], predicate: (item: T) => boolean): T[] => {
    return items.filter(item => predicate(item) && item !== null);
};

type Comparator<T> = (a: T, b: T) => a < b ? -1 : a > b ? 1 : 0;
]]></code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with CDATA TypeScript should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[1] {
        assert!(cb.content.contains("<T>"));
        assert!(cb.content.contains("=> boolean"));
        assert!(cb.content.contains("a < b"));
        assert!(cb.content.contains("a > b"));
    }
}

#[test]
fn test_llm_plan_with_sql_in_cdata() {
    // LLM correctly uses CDATA for SQL with comparison operators
    let xml = wrap_in_plan(
        r#"<paragraph>Create the database query:</paragraph>
<code-block language="sql"><![CDATA[
SELECT * FROM users
WHERE age >= 18 AND age < 65
AND created_at > '2024-01-01'
AND status <> 'deleted';
]]></code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with CDATA SQL should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    if let ContentElement::CodeBlock(cb) = &plan.steps[0].content.elements[1] {
        assert!(cb.content.contains("age >= 18"));
        assert!(cb.content.contains("age < 65"));
        assert!(cb.content.contains("status <> 'deleted'"));
    }
}

#[test]
fn test_llm_mistake_code_block_without_cdata_fails() {
    // LLM forgets CDATA for code block - this SHOULD fail
    let xml = wrap_in_plan(
        r#"<paragraph>Wrong approach:</paragraph>
<code-block language="rust">
fn compare(a: i32, b: i32) -> bool {
    a < b && b > 0
}
</code-block>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_err(),
        "Code block with unescaped < should fail: {:?}",
        result.ok()
    );
}

#[test]
fn test_llm_mistake_unescaped_inline_generic_fails() {
    // LLM forgets to escape inline generic - this SHOULD fail
    let xml = wrap_in_plan(r#"<paragraph>Use the Vec<String> type for the list.</paragraph>"#);

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_err(),
        "Unescaped inline generic should fail: {:?}",
        result.ok()
    );
}

#[test]
fn test_llm_plan_list_with_code_references() {
    // LLM creates a list with properly escaped code references
    let xml = wrap_in_plan(
        r#"<list type="unordered">
<item>Update <code>parse&lt;T&gt;</code> to handle errors</item>
<item>Ensure <code>count &gt; 0</code> before processing</item>
<item>Replace <code>&amp;&amp;</code> with explicit checks</item>
</list>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with list and escaped code should parse: {:?}",
        result.err()
    );
}

#[test]
fn test_llm_plan_mixed_cdata_and_escaped_content() {
    // LLM uses both CDATA (for code blocks) and escaping (for inline)
    let xml = wrap_in_plan(
        r#"<paragraph>The <code>validate&lt;T&gt;</code> function should check bounds:</paragraph>
<code-block language="rust"><![CDATA[
pub fn validate<T: Ord>(value: T, min: T, max: T) -> bool {
    value >= min && value <= max
}
]]></code-block>
<paragraph>Call it with <code>validate(x, 0, 100)</code> where <code>x &gt;= 0</code>.</paragraph>"#,
    );

    let result = validate_plan_xml(&xml);
    assert!(
        result.is_ok(),
        "Plan with mixed CDATA and escaped content should parse: {:?}",
        result.err()
    );
    let plan = result.unwrap();
    // Check that both inline escaped and CDATA content is preserved
    assert_eq!(plan.steps[0].content.elements.len(), 3);
}
