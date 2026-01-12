//! Stack-based guideline aggregation.

use crate::language_detector::ProjectStack;

use super::base::ReviewGuidelines;
use super::{functional, go, java, javascript, php, python, ruby, rust, systems};

impl ReviewGuidelines {
    /// Generate guidelines for a specific project stack.
    ///
    /// This method supports multi-language projects by:
    /// 1. Adding guidelines for the primary language
    /// 2. Adding guidelines for all secondary languages
    /// 3. Framework-specific guidelines are added by each language module
    pub(crate) fn for_stack(stack: &ProjectStack) -> Self {
        let mut guidelines = Self::default();

        // Add primary language guidelines.
        add_language_guidelines(&mut guidelines, &stack.primary_language, stack);

        // Add secondary language guidelines (important for multi-framework projects).
        // This enables projects like PHP + TypeScript + React to get all relevant guidelines.
        for lang in &stack.secondary_languages {
            add_language_guidelines(&mut guidelines, lang, stack);
        }

        guidelines
    }
}

/// Add guidelines for a specific language.
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
