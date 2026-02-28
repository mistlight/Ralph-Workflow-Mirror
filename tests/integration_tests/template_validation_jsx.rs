//! Integration test for template validation with JSX-style content.
//!
//! This test verifies that template validation correctly handles values
//! containing `{{}}` patterns (like JSX code) by using substitution logs
//! rather than regex scanning.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::prompts::Template;
use std::collections::HashMap;

#[test]
fn test_jsx_in_diff_no_false_positive() {
    with_default_timeout(|| {
        // This is the actual content from the bug report
        let diff_content = r"
+ const transformStyle = {
+   transformStyle: 'preserve-3d',
+   perspective: '1000px'
+ };
+ const cardStyle = {
+   transform: 'rotateY(45deg)',
+   style: {{ zIndex: 0 }}
+ };
";

        let template_content = "Review the following diff:\n\n{{DIFF}}";
        let template = Template::new(template_content);

        let variables = HashMap::from([("DIFF", diff_content.to_string())]);

        // Render with log
        let rendered = template
            .render_with_log("commit_message_xml", &variables, &HashMap::new())
            .unwrap();

        // The rendered content should contain the JSX code
        assert!(rendered.content.contains("{{ zIndex: 0 }}"));

        // The substitution log should show DIFF was substituted
        assert_eq!(rendered.log.substituted.len(), 1);
        assert_eq!(rendered.log.substituted[0].name, "DIFF");

        // The log should show completion (no missing variables)
        assert!(rendered.log.is_complete());
        assert!(rendered.log.unsubstituted.is_empty());

        // The old regex validation would fail here (false positive)
        // But log-based validation correctly passes
    });
}

#[test]
fn test_actual_missing_variable_detected() {
    with_default_timeout(|| {
        let template = Template::new("Review: {{DIFF}}\nAuthor: {{AUTHOR}}");
        let variables = HashMap::from([("DIFF", "some diff".to_string())]);
        // AUTHOR is missing

        let rendered = template
            .render_with_log("test_template", &variables, &HashMap::new())
            .unwrap();

        assert!(!rendered.log.is_complete());
        assert_eq!(rendered.log.unsubstituted, vec!["AUTHOR".to_string()]);
    });
}

#[test]
fn test_multiple_jsx_patterns_in_value() {
    with_default_timeout(|| {
        let code_content = r"
const Component = () => {
  const style1 = {{ zIndex: 0 }};
  const style2 = {{ opacity: 1 }};
  const style3 = {{ transform: 'scale(1)' }};
  return <div style={{ ...style1, ...style2, ...style3 }} />;
};
";

        let template = Template::new("Code:\n{{CODE}}");
        let variables = HashMap::from([("CODE", code_content.to_string())]);

        let rendered = template
            .render_with_log("test", &variables, &HashMap::new())
            .unwrap();

        // All the {{ }} patterns should be preserved in output
        assert!(rendered.content.contains("{{ zIndex: 0 }}"));
        assert!(rendered.content.contains("{{ opacity: 1 }}"));
        assert!(rendered.content.contains("{{ transform: 'scale(1)' }}"));

        // Log should show successful substitution
        assert!(rendered.log.is_complete());
        assert_eq!(rendered.log.substituted.len(), 1);
        assert_eq!(rendered.log.substituted[0].name, "CODE");
    });
}
