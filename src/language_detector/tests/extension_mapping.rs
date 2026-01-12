use super::super::extension_to_language;

#[test]
fn extension_to_language_covers_common_languages() {
    assert_eq!(extension_to_language("rs"), Some("Rust"));
    assert_eq!(extension_to_language("py"), Some("Python"));
    assert_eq!(extension_to_language("js"), Some("JavaScript"));
    assert_eq!(extension_to_language("ts"), Some("TypeScript"));
    assert_eq!(extension_to_language("go"), Some("Go"));
    assert_eq!(extension_to_language("java"), Some("Java"));
    assert_eq!(extension_to_language("rb"), Some("Ruby"));
    assert_eq!(extension_to_language("php"), Some("PHP"));
    assert_eq!(extension_to_language("yml"), Some("YAML"));
    assert_eq!(extension_to_language("yaml"), Some("YAML"));
    assert_eq!(extension_to_language("json"), Some("JSON"));
    assert_eq!(extension_to_language("md"), None);
}

#[test]
fn extension_matching_is_case_insensitive() {
    assert_eq!(extension_to_language("RS"), Some("Rust"));
    assert_eq!(extension_to_language("Py"), Some("Python"));
    assert_eq!(extension_to_language("JS"), Some("JavaScript"));
}

