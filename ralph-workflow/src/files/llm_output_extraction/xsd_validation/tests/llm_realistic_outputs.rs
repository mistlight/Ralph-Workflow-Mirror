    // =========================================================================
    // REALISTIC LLM OUTPUT TESTS FOR COMMIT MESSAGES
    // These test actual patterns that LLMs produce when following the prompts
    // =========================================================================

    #[test]
    fn test_llm_commit_with_generic_type_in_subject() {
        // LLM correctly escapes generic type in commit subject
        let xml = r"<ralph-commit>
<ralph-subject>feat: add <code>Result&lt;T, E&gt;</code> support</ralph-subject>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped generic should parse: {result:?}"
        );
        let elements = result.unwrap();
        assert!(elements.subject.contains("Result<T, E>"));
    }

    #[test]
    fn test_llm_commit_with_comparison_in_body() {
        // LLM correctly escapes comparison in commit body
        let xml = r"<ralph-commit>
<ralph-subject>fix: correct boundary check</ralph-subject>
<ralph-body>The condition <code>count &lt; 0</code> was incorrect. Changed to <code>count &lt;= 0</code> to handle zero case.</ralph-body>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped comparisons should parse: {result:?}"
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("count < 0"));
        assert!(body.contains("count <= 0"));
    }

    #[test]
    fn test_llm_commit_with_logical_operators_in_body() {
        // LLM correctly escapes logical operators
        let xml = r"<ralph-commit>
<ralph-subject>refactor: simplify condition</ralph-subject>
<ralph-body>Simplified <code>a &amp;&amp; b || c</code> to use helper function.</ralph-body>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with escaped logical operators should parse: {result:?}"
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a && b || c"));
    }

    #[test]
    fn test_llm_commit_with_detailed_body_describing_code() {
        // LLM uses detailed body format with code references
        let xml = r"<ralph-commit>
<ralph-subject>feat(parser): add generic parsing</ralph-subject>
<ralph-body-summary>Added generic <code>parse&lt;T&gt;</code> function.</ralph-body-summary>
<ralph-body-details>- Supports any type implementing <code>FromStr</code>
- Returns <code>Result&lt;T, ParseError&gt;</code>
- Handles cases where <code>input.len() &gt; 0</code></ralph-body-details>
<ralph-body-footer>Closes #456</ralph-body-footer>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with detailed body should parse: {result:?}"
        );
        let elements = result.unwrap();
        assert!(elements.body_summary.unwrap().contains("parse<T>"));
        let details = elements.body_details.unwrap();
        assert!(details.contains("Result<T, ParseError>"));
        assert!(details.contains("input.len() > 0"));
    }

    #[test]
    fn test_llm_commit_with_html_reference_in_body() {
        // LLM describes HTML-related changes
        let xml = r#"<ralph-commit>
<ralph-subject>fix(ui): correct template rendering</ralph-subject>
<ralph-body>Fixed the <code>&lt;div class="container"&gt;</code> element that was not rendering correctly when <code>count &gt; 10</code>.</ralph-body>
</ralph-commit>"#;

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Commit with HTML reference should parse: {result:?}"
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("<div class=\"container\">"));
        assert!(body.contains("count > 10"));
    }

    #[test]
    fn test_llm_mistake_unescaped_generic_in_subject_fails() {
        // LLM forgets to escape generic in subject - this SHOULD fail
        let xml = r"<ralph-commit>
<ralph-subject>feat: add Vec<String> support</ralph-subject>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped generic in subject should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_mistake_unescaped_comparison_in_body_fails() {
        // LLM forgets to escape comparison - this SHOULD fail
        let xml = r"<ralph-commit>
<ralph-subject>fix: correct comparison</ralph-subject>
<ralph-body>Changed a < b to a <= b.</ralph-body>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_err(),
            "Unescaped comparison in body should fail: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_llm_uses_cdata_in_body() {
        // LLM uses CDATA for complex code reference (valid alternative)
        let xml = r"<ralph-commit>
<ralph-subject>fix: handle edge case</ralph-subject>
<ralph-body>Fixed the case where <code><![CDATA[a < b && c > d]]></code> fails.</ralph-body>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "CDATA in body should be valid: {result:?}"
        );
        let elements = result.unwrap();
        assert!(elements.body.unwrap().contains("a < b && c > d"));
    }

    #[test]
    fn test_llm_commit_realistic_refactor_message() {
        // A realistic refactor commit message an LLM might produce
        let xml = r"<ralph-commit>
<ralph-subject>refactor(api): extract validation logic</ralph-subject>
<ralph-body>Extracted the validation logic from <code>handle_request&lt;T&gt;</code> into a separate
<code>validate&lt;T: Validate&gt;</code> function. This improves testability and allows
reuse across endpoints that check <code>input.size() &lt; MAX_SIZE</code>.</ralph-body>
</ralph-commit>";

        let result = validate_xml_against_xsd(xml);
        assert!(
            result.is_ok(),
            "Realistic refactor commit should parse: {result:?}"
        );
        let elements = result.unwrap();
        let body = elements.body.unwrap();
        assert!(body.contains("handle_request<T>"));
        assert!(body.contains("validate<T: Validate>"));
        assert!(body.contains("input.size() < MAX_SIZE"));
    }
