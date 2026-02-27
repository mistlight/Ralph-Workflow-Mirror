use super::*;

#[test]
fn test_truncate_prompt_small_content() {
    let logger = test_logger();
    let content = "This is a small prompt that fits within limits.";
    let result = truncate_prompt_if_needed(content, &logger);
    assert_eq!(result, content);
}

#[test]
fn test_truncate_prompt_large_content_with_marker() {
    let logger = test_logger();
    let prefix = "Task: Do something\n\n---\n";
    let large_content = "x".repeat(MAX_PROMPT_SIZE + 50000);
    let content = format!("{prefix}{large_content}");

    let result = truncate_prompt_if_needed(&content, &logger);

    assert!(result.len() < content.len());
    assert!(result.contains("truncated"));
    assert!(result.starts_with("Task:"));
}

#[test]
fn test_truncate_prompt_large_content_fallback() {
    let logger = test_logger();
    let content = "a".repeat(MAX_PROMPT_SIZE + 50000);

    let result = truncate_prompt_if_needed(&content, &logger);

    assert!(result.len() < content.len());
    assert!(result.contains("truncated"));
}

#[test]
fn test_truncate_prompt_preserves_end() {
    let logger = test_logger();
    let prefix = "Instructions\n\n---\n";
    let middle = "m".repeat(MAX_PROMPT_SIZE);
    let suffix = "\nIMPORTANT_END_MARKER";
    let content = format!("{prefix}{middle}{suffix}");

    let result = truncate_prompt_if_needed(&content, &logger);
    assert!(result.contains("IMPORTANT_END_MARKER"));
}
