//! Tests for step validation.
//!
//! These tests verify that the parser correctly validates step elements,
//! including number attributes, titles, types, priorities, and content.

use super::*;
use crate::files::llm_output_extraction::xsd_validation::XsdErrorType;

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
