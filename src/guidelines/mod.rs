//! Language-Specific Review Guidelines Module
//!
//! Provides tailored code review guidance based on the detected project stack.
//! These guidelines are incorporated into review prompts to help agents focus
//! on language-specific best practices, common pitfalls, and security concerns.
//!
//! ## Severity Classification
//!
//! Each check can be associated with a severity level for prioritized feedback:
//! - **Critical**: Must fix before merge (security vulnerabilities, data loss risks)
//! - **High**: Should fix before merge (bugs, significant issues)
//! - **Medium**: Should address (code quality, maintainability)
//! - **Low**: Nice to have (minor improvements)
//! - **Info**: Informational (suggestions, observations)
//!
//! ## Multi-Language Support
//!
//! This module supports multi-framework projects (e.g., PHP + React + TypeScript).
//! Guidelines are aggregated from:
//! 1. The primary language
//! 2. All secondary languages
//! 3. All detected frameworks

#![deny(unsafe_code)]

mod base;
mod functional;
mod go;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust;
mod systems;

// Re-export public types
pub(crate) use base::{CheckSeverity, ReviewGuidelines};

use crate::language_detector::ProjectStack;

impl ReviewGuidelines {
    /// Generate guidelines for a specific project stack
    ///
    /// This method supports multi-language projects by:
    /// 1. Adding guidelines for the primary language
    /// 2. Adding guidelines for all secondary languages
    /// 3. Framework-specific guidelines are added by each language module
    pub(crate) fn for_stack(stack: &ProjectStack) -> Self {
        let mut guidelines = Self::default();

        // Add primary language guidelines
        add_language_guidelines(&mut guidelines, &stack.primary_language, stack);

        // Add secondary language guidelines (important for multi-framework projects)
        // This enables projects like PHP + TypeScript + React to get all relevant guidelines
        for lang in &stack.secondary_languages {
            add_language_guidelines(&mut guidelines, lang, stack);
        }

        guidelines
    }
}

/// Add guidelines for a specific language
fn add_language_guidelines(
    guidelines: &mut ReviewGuidelines,
    language: &str,
    stack: &ProjectStack,
) {
    match language {
        "Rust" => rust::add_guidelines(guidelines, stack),
        "Python" => python::add_guidelines(guidelines, stack),
        "JavaScript" => javascript::add_javascript_guidelines(guidelines, stack),
        "TypeScript" => javascript::add_typescript_guidelines(guidelines, stack),
        "Go" => go::add_guidelines(guidelines, stack),
        "Java" => java::add_java_guidelines(guidelines, stack),
        "Kotlin" => java::add_kotlin_guidelines(guidelines, stack),
        "Ruby" => ruby::add_guidelines(guidelines, stack),
        "PHP" => php::add_guidelines(guidelines, stack),
        "C" | "C++" => systems::add_c_cpp_guidelines(guidelines),
        "C#" => systems::add_csharp_guidelines(guidelines),
        "Elixir" => functional::add_elixir_guidelines(guidelines),
        "Scala" => functional::add_scala_guidelines(guidelines),
        "Swift" => functional::add_swift_guidelines(guidelines),
        _ => {} // Use defaults for unknown languages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_guidelines() {
        let guidelines = ReviewGuidelines::default();
        assert!(!guidelines.quality_checks.is_empty());
        assert!(!guidelines.security_checks.is_empty());
    }

    #[test]
    fn test_rust_guidelines() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Actix".to_string()],
            has_tests: true,
            test_framework: Some("cargo test".to_string()),
            package_manager: Some("Cargo".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Rust-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("unwrap")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("unsafe")));
        // Should have Actix-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("extractors")));
    }

    #[test]
    fn test_python_django_guidelines() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Django".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Python-specific checks
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("PEP")));
        // Should have Django-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Django")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("CSRF")));
    }

    #[test]
    fn test_typescript_react_guidelines() {
        let stack = ProjectStack {
            primary_language: "TypeScript".to_string(),
            secondary_languages: vec!["JavaScript".to_string()],
            frameworks: vec!["React".to_string(), "Next.js".to_string()],
            has_tests: true,
            test_framework: Some("Jest".to_string()),
            package_manager: Some("npm".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have TypeScript checks
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("any")));
        // Should have React checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("hooks")));
        // Should have Next.js checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("SSR") || c.contains("rendering")));
    }

    #[test]
    fn test_go_guidelines() {
        let stack = ProjectStack {
            primary_language: "Go".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Gin".to_string()],
            has_tests: true,
            test_framework: Some("go test".to_string()),
            package_manager: Some("Go modules".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Go-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("error") || c.contains("golint")));
        assert!(guidelines.anti_patterns.iter().any(|c| c.contains("panic")));
    }

    #[test]
    fn test_format_for_prompt() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            ..Default::default()
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);
        let formatted = guidelines.format_for_prompt();

        assert!(formatted.contains("CODE QUALITY"));
        assert!(formatted.contains("SECURITY"));
        assert!(formatted.contains("AVOID"));
    }

    #[test]
    fn test_summary() {
        let guidelines = ReviewGuidelines::default();
        let summary = guidelines.summary();

        assert!(summary.contains("quality checks"));
        assert!(summary.contains("security checks"));
        assert!(summary.contains("anti-patterns"));
    }

    #[test]
    fn test_unknown_language_uses_defaults() {
        let stack = ProjectStack {
            primary_language: "Brainfuck".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: None,
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should still have default guidelines
        assert!(!guidelines.quality_checks.is_empty());
        assert!(!guidelines.security_checks.is_empty());
    }

    #[test]
    fn test_java_guidelines() {
        let stack = ProjectStack {
            primary_language: "Java".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: true,
            test_framework: Some("JUnit".to_string()),
            package_manager: Some("Maven".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Java-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Optional")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("PreparedStatement")));
        assert!(guidelines
            .anti_patterns
            .iter()
            .any(|c| c.contains("Exception") || c.contains("Throwable")));
    }

    #[test]
    fn test_ruby_guidelines() {
        let stack = ProjectStack {
            primary_language: "Ruby".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: Some("Bundler".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Ruby-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("rubocop") || c.contains("Ruby")));
        assert!(guidelines
            .anti_patterns
            .iter()
            .any(|c| c.contains("monkey patching")));
    }

    #[test]
    fn test_c_cpp_guidelines() {
        let stack = ProjectStack {
            primary_language: "C++".to_string(),
            secondary_languages: vec!["C".to_string()],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: None,
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have C/C++ security checks
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("buffer") || c.contains("bounds")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("overflow")));
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("RAII") || c.contains("smart pointer")));
    }

    #[test]
    fn test_csharp_guidelines() {
        let stack = ProjectStack {
            primary_language: "C#".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: Some("NuGet".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have C# specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("async/await") || c.contains("IDisposable")));
        assert!(guidelines
            .anti_patterns
            .iter()
            .any(|c| c.contains("async void")));
    }

    #[test]
    fn test_php_guidelines() {
        let stack = ProjectStack {
            primary_language: "PHP".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: Some("Composer".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have PHP-specific security checks
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("prepared statements") || c.contains("htmlspecialchars")));
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("PSR")));
    }

    #[test]
    fn test_kotlin_guidelines() {
        let stack = ProjectStack {
            primary_language: "Kotlin".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: Some("Gradle".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Kotlin-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("null safety") || c.contains("data class")));
        assert!(guidelines.anti_patterns.iter().any(|c| c.contains("!!")));
    }

    #[test]
    fn test_swift_guidelines() {
        let stack = ProjectStack {
            primary_language: "Swift".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: None,
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Swift-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("optional")));
        assert!(guidelines
            .anti_patterns
            .iter()
            .any(|c| c.contains("force unwrapping") || c.contains("!")));
    }

    #[test]
    fn test_elixir_guidelines() {
        let stack = ProjectStack {
            primary_language: "Elixir".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: Some("ExUnit".to_string()),
            package_manager: Some("Mix".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Elixir-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("pattern matching") || c.contains("pipe")));
        assert!(guidelines
            .idioms
            .iter()
            .any(|c| c.contains("crash") || c.contains("supervisor")));
    }

    #[test]
    fn test_scala_guidelines() {
        let stack = ProjectStack {
            primary_language: "Scala".to_string(),
            secondary_languages: vec![],
            frameworks: vec![],
            has_tests: false,
            test_framework: None,
            package_manager: None,
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Scala-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("immutable") || c.contains("Option")));
        assert!(guidelines
            .anti_patterns
            .iter()
            .any(|c| c.contains("mutable")));
    }

    #[test]
    fn test_fastapi_guidelines() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["FastAPI".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have FastAPI-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Pydantic")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("OAuth2") || c.contains("JWT")));
    }

    #[test]
    fn test_flask_guidelines() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Flask".to_string()],
            has_tests: false,
            test_framework: None,
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Flask-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Blueprint")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("SECRET_KEY")));
    }

    #[test]
    fn test_rails_guidelines() {
        let stack = ProjectStack {
            primary_language: "Ruby".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Rails".to_string()],
            has_tests: true,
            test_framework: Some("RSpec".to_string()),
            package_manager: Some("Bundler".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Rails-specific security checks
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("strong parameters") || c.contains("CSRF")));
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Rails conventions")));
    }

    #[test]
    fn test_spring_guidelines() {
        let stack = ProjectStack {
            primary_language: "Java".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Spring".to_string()],
            has_tests: true,
            test_framework: Some("JUnit".to_string()),
            package_manager: Some("Maven".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Spring-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("constructor injection")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("Spring Security") || c.contains("@Valid")));
    }

    #[test]
    fn test_nextjs_guidelines() {
        let stack = ProjectStack {
            primary_language: "TypeScript".to_string(),
            secondary_languages: vec!["JavaScript".to_string()],
            frameworks: vec!["Next.js".to_string()],
            has_tests: true,
            test_framework: Some("Jest".to_string()),
            package_manager: Some("npm".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have SSR framework guidelines
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("SSR") || c.contains("rendering") || c.contains("hydration")));
    }

    #[test]
    fn test_multiple_frameworks_combines_guidelines() {
        let stack = ProjectStack {
            primary_language: "TypeScript".to_string(),
            secondary_languages: vec!["JavaScript".to_string()],
            frameworks: vec!["React".to_string(), "Express".to_string()],
            has_tests: true,
            test_framework: Some("Jest".to_string()),
            package_manager: Some("npm".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have React-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("hooks")));

        // Should have Express-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("middleware")));
    }

    #[test]
    fn test_format_for_prompt_output() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            ..Default::default()
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);
        let formatted = guidelines.format_for_prompt();

        // Should contain section headers
        assert!(formatted.contains("CODE QUALITY:"));
        assert!(formatted.contains("SECURITY:"));

        // Should contain list items
        assert!(formatted.contains("  - "));

        // Should have reasonable length (not empty, not excessively long)
        assert!(formatted.len() > 100);
        assert!(formatted.len() < 5000);
    }

    #[test]
    fn test_get_all_checks_severity_distribution() {
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Django".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);
        let all_checks = guidelines.get_all_checks();

        // Count checks by severity
        let critical = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Critical)
            .count();
        let high = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::High)
            .count();
        let medium = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Medium)
            .count();
        let low = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Low)
            .count();

        // Severity distribution should make sense
        assert!(critical > 0, "Should have critical checks");
        assert!(high > 0, "Should have high severity checks");
        assert!(medium > 0, "Should have medium severity checks");
        assert!(low > 0, "Should have low severity checks");

        // Medium should typically have the most checks (quality, performance, etc.)
        assert!(
            medium >= high,
            "Medium should have at least as many checks as high"
        );
    }

    #[test]
    fn test_rust_web_framework_guidelines() {
        let stack = ProjectStack {
            primary_language: "Rust".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Axum".to_string()],
            has_tests: true,
            test_framework: Some("cargo test".to_string()),
            package_manager: Some("Cargo".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Rust web framework checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("extractors") || c.contains("async")));
    }

    #[test]
    fn test_go_web_framework_guidelines() {
        let stack = ProjectStack {
            primary_language: "Go".to_string(),
            secondary_languages: vec![],
            frameworks: vec!["Gin".to_string()],
            has_tests: true,
            test_framework: Some("go test".to_string()),
            package_manager: Some("Go modules".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Go web framework checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("handlers") || c.contains("context")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("CORS") || c.contains("input")));
    }

    // ============================================================================
    // Multi-language support tests (the key feature of this refactoring)
    // ============================================================================

    #[test]
    fn test_multi_language_php_react_typescript() {
        // Common in Laravel/Symfony projects with modern frontends
        let stack = ProjectStack {
            primary_language: "PHP".to_string(),
            secondary_languages: vec!["TypeScript".to_string(), "JavaScript".to_string()],
            frameworks: vec![
                "Laravel".to_string(),
                "React".to_string(),
                "Next.js".to_string(),
            ],
            has_tests: true,
            test_framework: Some("PHPUnit".to_string()),
            package_manager: Some("Composer".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have PHP-specific checks
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("PSR")));
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("prepared statements")));

        // Should have Laravel-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Eloquent") || c.contains("Laravel")));

        // Should have TypeScript checks from secondary language
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("any")));

        // Should have React checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("hooks")));

        // Should have Next.js checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("SSR") || c.contains("rendering")));
    }

    #[test]
    fn test_multi_language_python_javascript_vue() {
        // Common in Django/Flask with Vue frontends
        let stack = ProjectStack {
            primary_language: "Python".to_string(),
            secondary_languages: vec!["JavaScript".to_string()],
            frameworks: vec!["Django".to_string(), "Vue".to_string()],
            has_tests: true,
            test_framework: Some("pytest".to_string()),
            package_manager: Some("pip".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Python-specific checks
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("PEP")));

        // Should have Django-specific checks
        assert!(guidelines
            .security_checks
            .iter()
            .any(|c| c.contains("CSRF")));

        // Should have JavaScript checks from secondary language
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("const/let")));

        // Should have Vue checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Composition API")));
    }

    #[test]
    fn test_multi_language_java_typescript_angular() {
        // Spring Boot with Angular frontend
        let stack = ProjectStack {
            primary_language: "Java".to_string(),
            secondary_languages: vec!["TypeScript".to_string()],
            frameworks: vec!["Spring".to_string(), "Angular".to_string()],
            has_tests: true,
            test_framework: Some("JUnit".to_string()),
            package_manager: Some("Maven".to_string()),
        };

        let guidelines = ReviewGuidelines::for_stack(&stack);

        // Should have Java-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("Optional")));

        // Should have Spring-specific checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("constructor injection")));

        // Should have TypeScript checks from secondary language
        assert!(guidelines.quality_checks.iter().any(|c| c.contains("any")));

        // Should have Angular checks
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("OnPush") || c.contains("RxJS")));
    }
}
