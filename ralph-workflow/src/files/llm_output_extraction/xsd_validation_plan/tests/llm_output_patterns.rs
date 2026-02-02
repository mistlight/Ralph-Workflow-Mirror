//! Tests for realistic LLM output patterns.
//!
//! These tests verify that the parser correctly handles actual patterns that
//! LLMs produce when following the prompts, including proper CDATA usage for
//! code blocks and XML escaping for inline elements.

use super::*;

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
