//! Tests for minimal valid plan documents.
//!
//! These tests verify that the parser correctly handles the simplest valid plans
//! and comprehensive real-world plan documents.

use super::*;

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
    assert!(has_table, "Step 7 should have a table");

    // Steps 8-14: Various content sections, verify they have valid numbers and content
    for i in 7..14 {
        let step = &plan.steps[i];
        assert_eq!(step.number, (i + 1) as u32);
        assert!(!step.title.is_empty(), "Step {} should have a title", i + 1);
        assert!(
            !step.content.elements.is_empty(),
            "Step {} should have content",
            i + 1
        );
    }

    // Step 15: Final step
    let step15 = &plan.steps[14];
    assert_eq!(step15.number, 15);
    assert!(step15.priority.is_some(), "Step 15 should have a priority");
    assert!(!step15.title.is_empty(), "Step 15 should have a title");
    // Should have at least one dependency
    assert!(
        !step15.depends_on.is_empty(),
        "Final step should have at least one dependency"
    );

    // ═══════════════════════════════════════════════════════════════════════
    // CRITICAL FILES VALIDATION
    // ═══════════════════════════════════════════════════════════════════════

    // Should have primary files
    assert!(
        !plan.critical_files.primary_files.is_empty(),
        "Should have primary files"
    );

    // Primary file should be the main docs file
    let main_file = plan
        .critical_files
        .primary_files
        .iter()
        .find(|f| f.path.contains("website-design-spec"));
    assert!(main_file.is_some(), "Should have main spec file");
    assert_eq!(main_file.unwrap().action, FileAction::Modify);

    // Should have reference files
    assert!(
        !plan.critical_files.reference_files.is_empty(),
        "Should have reference files"
    );

    // ═══════════════════════════════════════════════════════════════════════
    // RISKS AND MITIGATIONS VALIDATION
    // ═══════════════════════════════════════════════════════════════════════

    // Should have multiple risk-mitigation pairs
    assert!(
        plan.risks_mitigations.len() >= 3,
        "Should have at least 3 risk-mitigation pairs"
    );

    // Check for different severity levels
    let has_high = plan
        .risks_mitigations
        .iter()
        .any(|r| r.severity == Some(Severity::High));
    let has_medium = plan
        .risks_mitigations
        .iter()
        .any(|r| r.severity == Some(Severity::Medium));
    assert!(has_high, "Should have at least one high severity risk");
    assert!(has_medium, "Should have at least one medium severity risk");

    // Each risk should have non-empty risk and mitigation text
    for rm in &plan.risks_mitigations {
        assert!(!rm.risk.is_empty(), "Risk text should not be empty");
        assert!(
            !rm.mitigation.is_empty(),
            "Mitigation text should not be empty"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // VERIFICATION STRATEGY VALIDATION
    // ═══════════════════════════════════════════════════════════════════════

    // Should have multiple verifications
    assert!(
        plan.verification_strategy.len() >= 2,
        "Should have at least 2 verification methods"
    );

    // Each verification should have method and expected outcome
    for v in &plan.verification_strategy {
        assert!(!v.method.is_empty(), "Method should not be empty");
        assert!(
            !v.expected_outcome.is_empty(),
            "Expected outcome should not be empty"
        );
    }

    // Check for specific verification types
    let methods: Vec<&str> = plan
        .verification_strategy
        .iter()
        .map(|v| v.method.as_str())
        .collect();
    let has_review = methods.iter().any(|m| m.to_lowercase().contains("review"));
    let has_test_or_build = methods
        .iter()
        .any(|m| m.to_lowercase().contains("test") || m.to_lowercase().contains("build"));
    assert!(has_review, "Should have a review-based verification");
    assert!(
        has_test_or_build,
        "Should have a test or build verification"
    );
}

#[test]
fn test_real_world_plan_error_messages_are_actionable() {
    // Simulate a common LLM mistake: file-change without target-files
    let xml = r#"<ralph-plan>
<ralph-summary>
<context>Add feature X</context>
<scope-items>
<scope-item count="3" category="files">files to modify</scope-item>
<scope-item count="1" category="feature">main feature</scope-item>
<scope-item count="5" category="tests">test cases</scope-item>
</scope-items>
</ralph-summary>
<ralph-implementation-steps>
<step number="1" type="file-change" priority="high">
<title>Add configuration</title>
<location>At the top of the config file</location>
<content>
<paragraph>Add the new configuration option.</paragraph>
<code-block language="rust">
pub struct NewConfig {
    pub enabled: bool,
}
</code-block>
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
<risk>Breaking changes</risk>
<mitigation>Add tests</mitigation>
</risk-pair>
</ralph-risks-mitigations>
<ralph-verification-strategy>
<verification>
<method>Run tests</method>
<expected-outcome>All pass</expected-outcome>
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
