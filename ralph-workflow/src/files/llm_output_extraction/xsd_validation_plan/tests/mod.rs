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
<item>First run <code>npm install</code> then:
<code-block language="bash">
npm run build
npm run test
</code-block>
After that, check <emphasis>logs</emphasis>.
</item>
<item>Deploy using:
<paragraph>Make sure to backup first!</paragraph>
<code-block language="bash">./deploy.sh</code-block>
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
        "List items with mixed content should be valid: {:?}",
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
fn test_list_item_with_nested_list_and_code_block() {
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Test nested lists with code blocks</context>
<scope-items>
<scope-item>item 1</scope-item>
<scope-item>item 2</scope-item>
<scope-item>item 3</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="action">
<title>Nested list with code</title>
<content>
<list type="ordered">
<item>Setup:
<code-block language="bash">mkdir project</code-block>
<list type="unordered">
<item>Create <code>src</code> folder</item>
<item>Create <code>tests</code> folder</item>
</list>
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
        "Nested lists with code blocks should be valid: {:?}",
        result.err()
    );

    let plan = result.unwrap();
    if let ContentElement::List(l) = &plan.steps[0].content.elements[0] {
        assert_eq!(l.items.len(), 1);
        // Should have a nested list
        assert!(l.items[0].nested_list.is_some());
        let nested = l.items[0].nested_list.as_ref().unwrap();
        assert_eq!(nested.items.len(), 2);
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_strip_block_elements_helper() {
    // Test the helper function directly
    let content = r#"First text <code-block language="rust">fn main() {}</code-block> middle <paragraph>para</paragraph> end"#;
    let stripped = super::strip_block_elements_for_inline_parsing(content);
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
    let stripped = super::strip_block_elements_for_inline_parsing(content);
    assert!(!stripped.contains("<list"), "Should strip list tags");
    assert!(stripped.contains("Text before"), "Should preserve text");
    assert!(stripped.contains("text after"), "Should preserve text");
}
