//! Tests for CDATA handling and XML character escaping.
//!
//! This module tests how the parser handles CDATA sections in code blocks
//! and proper XML escaping for special characters like <, >, and &.

use super::*;

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
