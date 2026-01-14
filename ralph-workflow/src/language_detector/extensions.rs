//! File extension to language mapping.
//!
//! Maps file extensions to programming language names.

/// Mapping from file extensions to language names
pub(crate) fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        // Rust
        "rs" => Some("Rust"),
        // Python
        "py" | "pyw" | "pyi" => Some("Python"),
        // JavaScript/TypeScript
        "js" | "mjs" | "cjs" => Some("JavaScript"),
        "ts" | "mts" | "cts" => Some("TypeScript"),
        "jsx" => Some("JavaScript"),
        "tsx" => Some("TypeScript"),
        // Go
        "go" => Some("Go"),
        // Java
        "java" => Some("Java"),
        // Kotlin
        "kt" | "kts" => Some("Kotlin"),
        // C/C++
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("C++"),
        // C#
        "cs" => Some("C#"),
        // Ruby
        "rb" | "erb" => Some("Ruby"),
        // PHP
        "php" => Some("PHP"),
        // Swift
        "swift" => Some("Swift"),
        // Scala
        "scala" | "sc" => Some("Scala"),
        // Shell
        "sh" | "bash" | "zsh" => Some("Shell"),
        // SQL
        "sql" => Some("SQL"),
        // Common "polyglot" repo file types
        "yml" | "yaml" => Some("YAML"),
        "json" => Some("JSON"),
        "html" | "htm" => Some("HTML"),
        "css" => Some("CSS"),
        "scss" => Some("SCSS"),
        "sass" => Some("Sass"),
        "less" => Some("Less"),
        // Lua
        "lua" => Some("Lua"),
        // Perl
        "pl" | "pm" => Some("Perl"),
        // R
        "r" => Some("R"),
        // Dart
        "dart" => Some("Dart"),
        // Elixir
        "ex" | "exs" => Some("Elixir"),
        // Haskell
        "hs" | "lhs" => Some("Haskell"),
        // OCaml
        "ml" | "mli" => Some("OCaml"),
        // F#
        "fs" | "fsi" | "fsx" => Some("F#"),
        // Clojure
        "clj" | "cljs" | "cljc" | "edn" => Some("Clojure"),
        // Zig
        "zig" => Some("Zig"),
        // Nim
        "nim" => Some("Nim"),
        // V
        "v" => Some("V"),
        _ => None,
    }
}

/// Check if a language is a non-primary/config language
/// that shouldn't be preferred as the primary language.
pub(super) fn is_non_primary_language(lang: &str) -> bool {
    matches!(
        lang,
        "YAML" | "JSON" | "HTML" | "CSS" | "SCSS" | "Sass" | "Less"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_to_language() {
        assert_eq!(extension_to_language("rs"), Some("Rust"));
        assert_eq!(extension_to_language("py"), Some("Python"));
        assert_eq!(extension_to_language("js"), Some("JavaScript"));
        assert_eq!(extension_to_language("ts"), Some("TypeScript"));
        assert_eq!(extension_to_language("go"), Some("Go"));
        assert_eq!(extension_to_language("java"), Some("Java"));
        assert_eq!(extension_to_language("rb"), Some("Ruby"));
        assert_eq!(extension_to_language("yml"), Some("YAML"));
        assert_eq!(extension_to_language("json"), Some("JSON"));
        assert_eq!(extension_to_language("html"), Some("HTML"));
        assert_eq!(extension_to_language("css"), Some("CSS"));
        assert_eq!(extension_to_language("unknown"), None);
    }

    #[test]
    fn test_is_non_primary_language() {
        assert!(is_non_primary_language("YAML"));
        assert!(is_non_primary_language("JSON"));
        assert!(is_non_primary_language("HTML"));
        assert!(is_non_primary_language("CSS"));
        assert!(!is_non_primary_language("Rust"));
        assert!(!is_non_primary_language("Python"));
    }
}
