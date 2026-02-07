// Tests for the init module.
//
// This file is included via include!() macro from the parent init.rs module.

use super::*;
use crate::config::MemoryConfigEnvironment;

/// Create a test environment with typical paths configured.
fn test_env() -> MemoryConfigEnvironment {
    MemoryConfigEnvironment::new()
        .with_unified_config_path("/test/config/ralph-workflow.toml")
        .with_prompt_path("/test/repo/PROMPT.md")
}

#[test]
fn test_handle_smart_init_with_valid_template_creates_prompt_md() {
    let env = test_env();
    let colors = Colors::new();

    let result = handle_smart_init_with(Some("quick"), false, colors, &env).unwrap();
    assert!(result);

    // Check prompt was created at the environment's prompt path
    let prompt_path = env.prompt_path();
    assert!(env.file_exists(&prompt_path));

    let template = get_template("quick").unwrap();
    let content = env.read_file(&prompt_path).unwrap();
    assert_eq!(content, template.content());
}

#[test]
fn test_handle_smart_init_with_invalid_template_does_not_create_prompt_md() {
    let env = test_env();
    let colors = Colors::new();

    let result = handle_smart_init_with(Some("nonexistent-template"), false, colors, &env).unwrap();
    assert!(result);

    // Prompt should not be created for invalid template
    let prompt_path = env.prompt_path();
    assert!(!env.file_exists(&prompt_path));
}

#[test]
fn test_template_name_validation() {
    // Test that we can validate template names
    assert!(get_template("bug-fix").is_some());
    assert!(get_template("feature-spec").is_some());
    assert!(get_template("refactor").is_some());
    assert!(get_template("test").is_some());
    assert!(get_template("docs").is_some());
    assert!(get_template("quick").is_some());

    // Invalid template names
    assert!(get_template("invalid").is_none());
    assert!(get_template("").is_none());
}

#[test]
fn test_levenshtein_distance() {
    // Exact match
    assert_eq!(levenshtein_distance("test", "test"), 0);

    // One edit
    assert_eq!(levenshtein_distance("test", "tast"), 1);
    assert_eq!(levenshtein_distance("test", "tests"), 1);
    assert_eq!(levenshtein_distance("test", "est"), 1);

    // Two edits
    assert_eq!(levenshtein_distance("test", "taste"), 2);
    assert_eq!(levenshtein_distance("test", "best"), 1);

    // Completely different
    assert_eq!(levenshtein_distance("abc", "xyz"), 3);
}

#[test]
fn test_similarity() {
    // Exact match
    assert_eq!(similarity_percentage("test", "test"), 100);

    // Similar strings - should be high similarity
    assert!(similarity_percentage("bug-fix", "bugfix") > 80);
    assert!(similarity_percentage("feature-spec", "feature") > 50);

    // Different strings - should be low similarity
    assert!(similarity_percentage("test", "xyz") < 50);

    // Empty strings
    assert_eq!(similarity_percentage("", ""), 100);
    assert_eq!(similarity_percentage("test", ""), 0);
    assert_eq!(similarity_percentage("", "test"), 0);
}

#[test]
fn test_find_similar_templates() {
    // Find similar to "bugfix" (missing hyphen)
    let similar = find_similar_templates("bugfix");
    assert!(!similar.is_empty());
    assert!(similar.iter().any(|(name, _)| *name == "bug-fix"));

    // Find similar to "feature" (should match feature-spec)
    let similar = find_similar_templates("feature");
    assert!(!similar.is_empty());
    assert!(similar.iter().any(|(name, _)| name.contains("feature")));

    // Very different string should return empty or low similarity
    let similar = find_similar_templates("xyzabc");
    // Either empty or all matches have low similarity
    assert!(similar.is_empty() || similar.iter().all(|(_, sim)| *sim < 50));
}

#[test]
fn test_init_local_config_creates_file() {
    let env = test_env().with_local_config_path("/test/project/.agent/ralph-workflow.toml");

    let result = handle_init_local_config_with(Colors::new(), &env, false);

    assert!(result.is_ok());
    assert!(env.was_written(std::path::Path::new(
        "/test/project/.agent/ralph-workflow.toml"
    )));

    let content = env
        .get_file(std::path::Path::new(
            "/test/project/.agent/ralph-workflow.toml",
        ))
        .unwrap();
    assert!(content.contains("Local Ralph configuration"));
    assert!(content.contains("developer_iters"));
}

#[test]
fn test_init_local_config_refuses_overwrite_without_force() {
    let env = test_env()
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file(
            "/test/project/.agent/ralph-workflow.toml",
            "existing content",
        );

    let result = handle_init_local_config_with(Colors::new(), &env, false);

    assert!(result.is_ok());
    // Content should be unchanged
    assert_eq!(
        env.get_file(std::path::Path::new(
            "/test/project/.agent/ralph-workflow.toml"
        )),
        Some("existing content".to_string())
    );
}

#[test]
fn test_init_local_config_overwrites_with_force() {
    let env = test_env()
        .with_local_config_path("/test/project/.agent/ralph-workflow.toml")
        .with_file(
            "/test/project/.agent/ralph-workflow.toml",
            "existing content",
        );

    let result = handle_init_local_config_with(Colors::new(), &env, true);

    assert!(result.is_ok());
    let content = env
        .get_file(std::path::Path::new(
            "/test/project/.agent/ralph-workflow.toml",
        ))
        .unwrap();
    assert!(content.contains("Local Ralph configuration"));
}
