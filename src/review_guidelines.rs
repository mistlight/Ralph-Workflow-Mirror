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

#![deny(unsafe_code)]

use crate::language_detector::ProjectStack;

/// Severity level for code review checks
///
/// Used to prioritize review feedback and help developers focus on
/// the most important issues first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CheckSeverity {
    /// Must fix before merge - security vulnerabilities, data loss, crashes
    Critical,
    /// Should fix before merge - bugs, significant functional issues
    High,
    /// Should address - code quality, maintainability concerns
    Medium,
    /// Nice to have - minor improvements, style suggestions
    Low,
    /// Informational - observations, suggestions for future consideration
    Info,
}

impl std::fmt::Display for CheckSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckSeverity::Critical => write!(f, "CRITICAL"),
            CheckSeverity::High => write!(f, "HIGH"),
            CheckSeverity::Medium => write!(f, "MEDIUM"),
            CheckSeverity::Low => write!(f, "LOW"),
            CheckSeverity::Info => write!(f, "INFO"),
        }
    }
}

/// A review check with associated severity
#[derive(Debug, Clone)]
pub(crate) struct SeverityCheck {
    /// The check description
    /// Note: populated for future use (e.g., displaying detailed check info)
    #[allow(dead_code)]
    pub(crate) check: String,
    /// Severity level for this check
    pub(crate) severity: CheckSeverity,
}

impl SeverityCheck {
    fn new(check: impl Into<String>, severity: CheckSeverity) -> Self {
        Self {
            check: check.into(),
            severity,
        }
    }

    fn critical(check: impl Into<String>) -> Self {
        Self::new(check, CheckSeverity::Critical)
    }

    fn high(check: impl Into<String>) -> Self {
        Self::new(check, CheckSeverity::High)
    }

    fn medium(check: impl Into<String>) -> Self {
        Self::new(check, CheckSeverity::Medium)
    }

    fn low(check: impl Into<String>) -> Self {
        Self::new(check, CheckSeverity::Low)
    }

    #[allow(dead_code)]
    fn info(check: impl Into<String>) -> Self {
        Self::new(check, CheckSeverity::Info)
    }
}

/// Review guidelines for a specific technology stack
#[derive(Debug, Clone)]
pub(crate) struct ReviewGuidelines {
    /// Language-specific code quality checks
    pub(crate) quality_checks: Vec<String>,
    /// Security considerations specific to this stack
    pub(crate) security_checks: Vec<String>,
    /// Performance considerations
    pub(crate) performance_checks: Vec<String>,
    /// Testing expectations
    pub(crate) testing_checks: Vec<String>,
    /// Documentation requirements
    pub(crate) documentation_checks: Vec<String>,
    /// Common idioms and patterns to follow
    pub(crate) idioms: Vec<String>,
    /// Anti-patterns to avoid
    pub(crate) anti_patterns: Vec<String>,
    /// Concurrency and thread safety checks
    pub(crate) concurrency_checks: Vec<String>,
    /// Resource management checks (file handles, connections, memory)
    pub(crate) resource_checks: Vec<String>,
    /// Logging and observability checks
    pub(crate) observability_checks: Vec<String>,
    /// Configuration and secrets management checks
    pub(crate) secrets_checks: Vec<String>,
    /// API design checks (for libraries/services)
    pub(crate) api_design_checks: Vec<String>,
}

impl Default for ReviewGuidelines {
    fn default() -> Self {
        Self {
            quality_checks: vec![
                "Code follows consistent style and formatting".to_string(),
                "Functions have single responsibility".to_string(),
                "Error handling is comprehensive".to_string(),
                "No dead code or unused imports".to_string(),
            ],
            security_checks: vec![
                "No hardcoded secrets or credentials".to_string(),
                "Input validation on external data".to_string(),
                "Proper authentication/authorization checks".to_string(),
            ],
            performance_checks: vec![
                "No obvious performance bottlenecks".to_string(),
                "Efficient data structures used".to_string(),
            ],
            testing_checks: vec![
                "Tests cover main functionality".to_string(),
                "Edge cases are tested".to_string(),
            ],
            documentation_checks: vec![
                "Public APIs are documented".to_string(),
                "Complex logic has explanatory comments".to_string(),
            ],
            idioms: vec!["Code follows language conventions".to_string()],
            anti_patterns: vec!["Avoid code duplication".to_string()],
            concurrency_checks: vec![
                "Shared mutable state is properly synchronized".to_string(),
                "No potential deadlocks (lock ordering)".to_string(),
            ],
            resource_checks: vec![
                "Resources are properly closed/released".to_string(),
                "No resource leaks in error paths".to_string(),
            ],
            observability_checks: vec![
                "Errors are logged with context".to_string(),
                "Critical operations have appropriate logging".to_string(),
            ],
            secrets_checks: vec![
                "Secrets loaded from environment/config, not hardcoded".to_string(),
                "Sensitive data not logged or exposed in errors".to_string(),
            ],
            api_design_checks: vec![
                "API follows consistent naming conventions".to_string(),
                "Breaking changes are clearly documented".to_string(),
            ],
        }
    }
}

impl ReviewGuidelines {
    /// Generate guidelines for a specific project stack
    pub(crate) fn for_stack(stack: &ProjectStack) -> Self {
        let mut guidelines = Self::default();

        // Add language-specific guidelines
        match stack.primary_language.as_str() {
            "Rust" => guidelines.add_rust_guidelines(),
            "Python" => guidelines.add_python_guidelines(),
            "JavaScript" => guidelines.add_javascript_guidelines(stack),
            "TypeScript" => guidelines.add_typescript_guidelines(stack),
            "Go" => guidelines.add_go_guidelines(),
            "Java" => guidelines.add_java_guidelines(stack),
            "Ruby" => guidelines.add_ruby_guidelines(stack),
            "C" | "C++" => guidelines.add_c_cpp_guidelines(),
            "C#" => guidelines.add_csharp_guidelines(),
            "PHP" => guidelines.add_php_guidelines(stack),
            "Kotlin" => guidelines.add_kotlin_guidelines(),
            "Swift" => guidelines.add_swift_guidelines(),
            "Elixir" => guidelines.add_elixir_guidelines(),
            "Scala" => guidelines.add_scala_guidelines(),
            _ => {} // Use defaults
        }

        // Add framework-specific guidelines
        for framework in &stack.frameworks {
            match framework.as_str() {
                "React" => guidelines.add_react_guidelines(),
                "Vue" => guidelines.add_vue_guidelines(),
                "Angular" => guidelines.add_angular_guidelines(),
                "Django" => guidelines.add_django_guidelines(),
                "FastAPI" => guidelines.add_fastapi_guidelines(),
                "Flask" => guidelines.add_flask_guidelines(),
                "Rails" => guidelines.add_rails_guidelines(),
                "Spring" => guidelines.add_spring_guidelines(),
                "Express" | "Fastify" | "NestJS" => guidelines.add_node_backend_guidelines(),
                "Next.js" | "Nuxt" => guidelines.add_ssr_framework_guidelines(),
                "Actix" | "Axum" | "Rocket" => guidelines.add_rust_web_guidelines(),
                "Gin" | "Chi" | "Fiber" | "Echo" => guidelines.add_go_web_guidelines(),
                _ => {}
            }
        }

        guidelines
    }

    fn add_rust_guidelines(&mut self) {
        self.quality_checks.extend([
            "No unwrap/expect in production paths; use Result + ?".to_string(),
            "Proper lifetime annotations where needed".to_string(),
            "Prefer borrowing over cloning".to_string(),
            "Use strong types and exhaustive matching".to_string(),
            "Keep public API minimal (pub(crate) by default)".to_string(),
        ]);
        self.security_checks.extend([
            "Minimize unsafe code blocks; justify each use".to_string(),
            "Check for integer overflow in arithmetic".to_string(),
            "Validate untrusted input before processing".to_string(),
        ]);
        self.performance_checks.extend([
            "Avoid unnecessary allocations (String → &str, Vec → slice)".to_string(),
            "Use iterators instead of indexing loops".to_string(),
            "Consider async for I/O-bound operations".to_string(),
        ]);
        self.testing_checks.extend([
            "Unit tests for core logic (#[cfg(test)])".to_string(),
            "Integration tests in tests/ directory".to_string(),
            "Consider property-based testing for invariants".to_string(),
        ]);
        self.idioms.extend([
            "Follow Rust API Guidelines".to_string(),
            "Use derive macros appropriately".to_string(),
            "Implement standard traits (Debug, Clone, etc.)".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid .clone() to satisfy borrow checker without understanding".to_string(),
            "Don't use Rc<RefCell<T>> when ownership can be restructured".to_string(),
            "Avoid panic! in library code".to_string(),
        ]);
    }

    fn add_python_guidelines(&mut self) {
        self.quality_checks.extend([
            "Follow PEP 8 style guide".to_string(),
            "Use type hints for function signatures".to_string(),
            "Prefer f-strings over .format()".to_string(),
            "Use context managers for resources".to_string(),
        ]);
        self.security_checks.extend([
            "No eval() or exec() with untrusted input".to_string(),
            "Use parameterized queries for database operations".to_string(),
            "Validate file paths to prevent path traversal".to_string(),
            "Check pickle/yaml.load usage for untrusted data".to_string(),
        ]);
        self.performance_checks.extend([
            "Use generators for large data processing".to_string(),
            "Consider list comprehensions over loops".to_string(),
            "Profile before optimizing".to_string(),
        ]);
        self.testing_checks.extend([
            "Use pytest fixtures for test setup".to_string(),
            "Mock external dependencies".to_string(),
            "Test exception handling".to_string(),
        ]);
        self.idioms.extend([
            "Use Pythonic idioms (EAFP over LBYL)".to_string(),
            "Leverage standard library (itertools, collections)".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid mutable default arguments".to_string(),
            "Don't use bare except clauses".to_string(),
            "Avoid global state".to_string(),
        ]);
    }

    fn add_javascript_guidelines(&mut self, stack: &ProjectStack) {
        self.quality_checks.extend([
            "Use const/let, never var".to_string(),
            "Handle Promise rejections".to_string(),
            "Use async/await over raw Promises".to_string(),
            "Avoid deeply nested callbacks".to_string(),
        ]);
        self.security_checks.extend([
            "Sanitize user input before DOM insertion".to_string(),
            "Use Content Security Policy headers".to_string(),
            "Validate data from external APIs".to_string(),
            "Check for prototype pollution vulnerabilities".to_string(),
        ]);
        self.performance_checks.extend([
            "Debounce/throttle frequent event handlers".to_string(),
            "Use appropriate data structures".to_string(),
            "Minimize DOM manipulation".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid == for comparisons (use ===)".to_string(),
            "Don't mutate function arguments".to_string(),
            "Avoid synchronous I/O in Node.js".to_string(),
        ]);

        if stack.frameworks.iter().any(|f| f == "React" || f == "Vue") {
            self.add_frontend_guidelines();
        }
    }

    fn add_typescript_guidelines(&mut self, stack: &ProjectStack) {
        self.add_javascript_guidelines(stack);
        self.quality_checks.extend([
            "Use strict TypeScript mode".to_string(),
            "Prefer interfaces over type aliases for objects".to_string(),
            "Use explicit return types for public functions".to_string(),
            "Avoid 'any' type; use 'unknown' if needed".to_string(),
        ]);
        self.idioms.extend([
            "Use union types for discriminated unions".to_string(),
            "Leverage type inference where clear".to_string(),
            "Use generics appropriately".to_string(),
        ]);
        self.anti_patterns.extend([
            "Don't use 'as' casts to bypass type checking".to_string(),
            "Avoid non-null assertions (!) without justification".to_string(),
        ]);
    }

    fn add_go_guidelines(&mut self) {
        self.quality_checks.extend([
            "Run go fmt and golint".to_string(),
            "Check all error returns".to_string(),
            "Use defer for cleanup".to_string(),
            "Keep functions short and focused".to_string(),
        ]);
        self.security_checks.extend([
            "Validate input bounds before slice operations".to_string(),
            "Use crypto/rand for security-sensitive random numbers".to_string(),
            "Check for SQL injection in database queries".to_string(),
        ]);
        self.performance_checks.extend([
            "Pre-allocate slices when size is known".to_string(),
            "Use sync.Pool for frequently allocated objects".to_string(),
            "Consider goroutine leaks".to_string(),
        ]);
        self.testing_checks.extend([
            "Use table-driven tests".to_string(),
            "Test error paths explicitly".to_string(),
            "Use testify or similar for assertions".to_string(),
        ]);
        self.idioms.extend([
            "Accept interfaces, return structs".to_string(),
            "Make the zero value useful".to_string(),
            "Don't communicate by sharing memory".to_string(),
        ]);
        self.anti_patterns.extend([
            "Don't ignore returned errors".to_string(),
            "Avoid init() when possible".to_string(),
            "Don't use panic for normal error handling".to_string(),
        ]);
    }

    fn add_java_guidelines(&mut self, stack: &ProjectStack) {
        self.quality_checks.extend([
            "Follow Java naming conventions".to_string(),
            "Use Optional instead of null returns".to_string(),
            "Prefer composition over inheritance".to_string(),
            "Use try-with-resources for AutoCloseable".to_string(),
        ]);
        self.security_checks.extend([
            "Use PreparedStatement for SQL queries".to_string(),
            "Validate deserialized objects".to_string(),
            "Check for path traversal in file operations".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid catching Exception or Throwable".to_string(),
            "Don't use raw types with generics".to_string(),
            "Avoid public fields".to_string(),
        ]);

        if stack.frameworks.contains(&"Spring".to_string()) {
            self.add_spring_guidelines();
        }
    }

    fn add_ruby_guidelines(&mut self, stack: &ProjectStack) {
        self.quality_checks.extend([
            "Follow Ruby style guide (rubocop)".to_string(),
            "Use meaningful variable names".to_string(),
            "Keep methods under 10 lines when possible".to_string(),
            "Use symbols instead of strings for keys".to_string(),
        ]);
        self.security_checks.extend([
            "Use parameterized queries (avoid string interpolation in SQL)".to_string(),
            "Escape output in views".to_string(),
            "Validate strong parameters".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid monkey patching core classes".to_string(),
            "Don't use eval with user input".to_string(),
            "Avoid deeply nested conditionals".to_string(),
        ]);

        if stack.frameworks.contains(&"Rails".to_string()) {
            self.add_rails_guidelines();
        }
    }

    fn add_c_cpp_guidelines(&mut self) {
        self.quality_checks.extend([
            "Check return values of system calls".to_string(),
            "Use RAII for resource management (C++)".to_string(),
            "Prefer smart pointers over raw pointers (C++)".to_string(),
            "Initialize all variables".to_string(),
        ]);
        self.security_checks.extend([
            "Check buffer bounds before operations".to_string(),
            "Use safe string functions (strncpy, snprintf)".to_string(),
            "Validate array indices".to_string(),
            "Check for integer overflow".to_string(),
            "Avoid use-after-free".to_string(),
        ]);
        self.performance_checks.extend([
            "Minimize memory allocations in hot paths".to_string(),
            "Use const references for large objects".to_string(),
            "Consider cache locality".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid raw new/delete (C++)".to_string(),
            "Don't use C-style casts (C++)".to_string(),
            "Avoid global mutable state".to_string(),
        ]);
    }

    fn add_csharp_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use async/await for I/O operations".to_string(),
            "Implement IDisposable correctly".to_string(),
            "Use nullable reference types".to_string(),
            "Follow C# naming conventions".to_string(),
        ]);
        self.security_checks.extend([
            "Use parameterized queries with Entity Framework".to_string(),
            "Validate model binding input".to_string(),
            "Use HTTPS and proper authentication".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid async void (except event handlers)".to_string(),
            "Don't catch generic Exception".to_string(),
            "Avoid blocking on async code".to_string(),
        ]);
    }

    fn add_php_guidelines(&mut self, stack: &ProjectStack) {
        self.quality_checks.extend([
            "Use PHP 8+ features where available".to_string(),
            "Follow PSR standards".to_string(),
            "Use type declarations".to_string(),
            "Use named arguments for clarity".to_string(),
        ]);
        self.security_checks.extend([
            "Use prepared statements for database queries".to_string(),
            "Escape output with htmlspecialchars()".to_string(),
            "Validate file uploads thoroughly".to_string(),
            "Use password_hash() for passwords".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid using extract() with user input".to_string(),
            "Don't suppress errors with @".to_string(),
            "Avoid register_globals behavior".to_string(),
        ]);

        if stack.frameworks.contains(&"Laravel".to_string()) {
            self.quality_checks
                .push("Use Eloquent relationships properly".to_string());
            self.security_checks
                .push("Use Laravel's CSRF protection".to_string());
        }
    }

    fn add_kotlin_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use null safety features".to_string(),
            "Prefer data classes for DTOs".to_string(),
            "Use extension functions appropriately".to_string(),
            "Leverage scope functions (let, run, apply)".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid !! operator without validation".to_string(),
            "Don't use lateinit for nullable fields".to_string(),
        ]);
    }

    fn add_swift_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use optionals correctly".to_string(),
            "Follow Swift API design guidelines".to_string(),
            "Use value types where appropriate".to_string(),
            "Leverage Swift's type inference".to_string(),
        ]);
        self.security_checks.extend([
            "Use Keychain for sensitive data".to_string(),
            "Validate URL schemes".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid force unwrapping (!)".to_string(),
            "Don't use implicitly unwrapped optionals unnecessarily".to_string(),
        ]);
    }

    fn add_elixir_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use pattern matching effectively".to_string(),
            "Follow pipe operator conventions".to_string(),
            "Use dialyzer for type checking".to_string(),
        ]);
        self.performance_checks.extend([
            "Use streams for large data processing".to_string(),
            "Consider GenServer state design".to_string(),
        ]);
        self.idioms.extend([
            "Let it crash - use supervisors".to_string(),
            "Use with for happy path chaining".to_string(),
        ]);
    }

    fn add_scala_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use immutable collections".to_string(),
            "Prefer Option over null".to_string(),
            "Use pattern matching".to_string(),
            "Follow functional programming principles".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid mutable state".to_string(),
            "Don't use return statements".to_string(),
            "Avoid throwing exceptions".to_string(),
        ]);
    }

    fn add_frontend_guidelines(&mut self) {
        self.quality_checks.extend([
            "Components are properly modularized".to_string(),
            "State management is predictable".to_string(),
            "Accessibility (a11y) is considered".to_string(),
        ]);
        self.performance_checks.extend([
            "Avoid unnecessary re-renders".to_string(),
            "Use lazy loading for large components".to_string(),
            "Optimize bundle size".to_string(),
        ]);
    }

    fn add_react_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use hooks correctly (rules of hooks)".to_string(),
            "Properly manage component lifecycle".to_string(),
            "Use React.memo for expensive renders".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid prop drilling (use context or state management)".to_string(),
            "Don't mutate state directly".to_string(),
            "Avoid inline functions in render".to_string(),
        ]);
    }

    fn add_vue_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use Composition API for complex logic".to_string(),
            "Follow Vue style guide".to_string(),
            "Use computed properties appropriately".to_string(),
        ]);
        self.anti_patterns.extend([
            "Avoid watchers when computed works".to_string(),
            "Don't directly mutate props".to_string(),
        ]);
    }

    fn add_angular_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use OnPush change detection where possible".to_string(),
            "Follow Angular style guide".to_string(),
            "Use RxJS operators effectively".to_string(),
        ]);
        self.security_checks
            .push("Use Angular's built-in sanitization".to_string());
        self.anti_patterns.extend([
            "Avoid subscribing without unsubscribing".to_string(),
            "Don't use any type".to_string(),
        ]);
    }

    fn add_django_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use Django ORM effectively".to_string(),
            "Follow Django coding style".to_string(),
            "Use class-based views appropriately".to_string(),
        ]);
        self.security_checks.extend([
            "Use Django's CSRF protection".to_string(),
            "Validate forms properly".to_string(),
            "Use Django's authentication system".to_string(),
        ]);
    }

    fn add_fastapi_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use Pydantic models for validation".to_string(),
            "Define proper response models".to_string(),
            "Use dependency injection".to_string(),
        ]);
        self.security_checks.extend([
            "Implement proper OAuth2/JWT handling".to_string(),
            "Use HTTPSRedirectMiddleware".to_string(),
        ]);
    }

    fn add_flask_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use Blueprints for organization".to_string(),
            "Use Flask-SQLAlchemy properly".to_string(),
        ]);
        self.security_checks.extend([
            "Set SECRET_KEY securely".to_string(),
            "Use flask-talisman for security headers".to_string(),
        ]);
    }

    fn add_rails_guidelines(&mut self) {
        self.quality_checks.extend([
            "Follow Rails conventions".to_string(),
            "Use Active Record validations".to_string(),
            "Keep controllers thin".to_string(),
        ]);
        self.security_checks.extend([
            "Use strong parameters".to_string(),
            "Protect against mass assignment".to_string(),
            "Use Rails' built-in CSRF protection".to_string(),
        ]);
    }

    fn add_spring_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use constructor injection".to_string(),
            "Follow Spring Boot conventions".to_string(),
            "Use proper transaction management".to_string(),
        ]);
        self.security_checks.extend([
            "Configure Spring Security properly".to_string(),
            "Use @Valid for input validation".to_string(),
        ]);
    }

    fn add_node_backend_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use middleware pattern effectively".to_string(),
            "Handle errors in middleware".to_string(),
            "Use environment variables for config".to_string(),
        ]);
        self.security_checks.extend([
            "Use helmet for security headers".to_string(),
            "Implement rate limiting".to_string(),
            "Validate request body schema".to_string(),
        ]);
    }

    fn add_ssr_framework_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use appropriate rendering strategy (SSR/SSG/ISR)".to_string(),
            "Handle hydration correctly".to_string(),
            "Optimize for Core Web Vitals".to_string(),
        ]);
        self.performance_checks.extend([
            "Minimize client-side JavaScript".to_string(),
            "Use image optimization".to_string(),
        ]);
    }

    fn add_rust_web_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use extractors for request data".to_string(),
            "Handle errors with proper status codes".to_string(),
            "Use async handlers appropriately".to_string(),
        ]);
        self.security_checks.extend([
            "Validate all user input".to_string(),
            "Use tower middleware for common concerns".to_string(),
        ]);
    }

    fn add_go_web_guidelines(&mut self) {
        self.quality_checks.extend([
            "Use proper error handling in handlers".to_string(),
            "Use context for cancellation".to_string(),
            "Structure handlers and middleware properly".to_string(),
        ]);
        self.security_checks.extend([
            "Set proper CORS headers".to_string(),
            "Validate input in handlers".to_string(),
        ]);
    }

    /// Format guidelines as a prompt section
    pub(crate) fn format_for_prompt(&self) -> String {
        let mut sections = Vec::new();

        if !self.quality_checks.is_empty() {
            let items: Vec<String> = self
                .quality_checks
                .iter()
                .take(5)
                .map(|s| format!("  - {}", s))
                .collect();
            sections.push(format!("CODE QUALITY:\n{}", items.join("\n")));
        }

        if !self.security_checks.is_empty() {
            let items: Vec<String> = self
                .security_checks
                .iter()
                .take(5)
                .map(|s| format!("  - {}", s))
                .collect();
            sections.push(format!("SECURITY:\n{}", items.join("\n")));
        }

        if !self.performance_checks.is_empty() {
            let items: Vec<String> = self
                .performance_checks
                .iter()
                .take(3)
                .map(|s| format!("  - {}", s))
                .collect();
            sections.push(format!("PERFORMANCE:\n{}", items.join("\n")));
        }

        if !self.anti_patterns.is_empty() {
            let items: Vec<String> = self
                .anti_patterns
                .iter()
                .take(3)
                .map(|s| format!("  - {}", s))
                .collect();
            sections.push(format!("AVOID:\n{}", items.join("\n")));
        }

        sections.join("\n\n")
    }

    /// Get all checks with their severity classifications
    ///
    /// Returns a comprehensive list of all applicable checks organized by category
    /// with severity levels. This is useful for generating detailed review reports.
    pub(crate) fn get_all_checks(&self) -> Vec<SeverityCheck> {
        let mut checks = Vec::new();

        // Security checks are CRITICAL
        for check in &self.security_checks {
            checks.push(SeverityCheck::critical(check.clone()));
        }
        for check in &self.secrets_checks {
            checks.push(SeverityCheck::critical(check.clone()));
        }

        // Concurrency issues are HIGH severity
        for check in &self.concurrency_checks {
            checks.push(SeverityCheck::high(check.clone()));
        }

        // Resource leaks and quality issues are MEDIUM
        for check in &self.resource_checks {
            checks.push(SeverityCheck::high(check.clone()));
        }
        for check in &self.quality_checks {
            checks.push(SeverityCheck::medium(check.clone()));
        }
        for check in &self.anti_patterns {
            checks.push(SeverityCheck::medium(check.clone()));
        }

        // Performance, testing, API design are MEDIUM to LOW
        for check in &self.performance_checks {
            checks.push(SeverityCheck::medium(check.clone()));
        }
        for check in &self.testing_checks {
            checks.push(SeverityCheck::medium(check.clone()));
        }
        for check in &self.api_design_checks {
            checks.push(SeverityCheck::medium(check.clone()));
        }

        // Observability and documentation are LOW
        for check in &self.observability_checks {
            checks.push(SeverityCheck::low(check.clone()));
        }
        for check in &self.documentation_checks {
            checks.push(SeverityCheck::low(check.clone()));
        }

        // Idioms are informational/LOW
        for check in &self.idioms {
            checks.push(SeverityCheck::low(check.clone()));
        }

        checks
    }

    /// Format guidelines with severity priorities for the review prompt
    ///
    /// This produces a more detailed prompt section that groups checks by priority,
    /// helping agents focus on the most critical issues first.
    pub(crate) fn format_for_prompt_with_priorities(&self) -> String {
        let mut sections = Vec::new();

        // Critical: Security and secrets
        let critical_checks: Vec<&str> = self
            .security_checks
            .iter()
            .chain(self.secrets_checks.iter())
            .take(5)
            .map(String::as_str)
            .collect();
        if !critical_checks.is_empty() {
            let items: Vec<String> = critical_checks
                .iter()
                .map(|s| format!("  - {}", s))
                .collect();
            sections.push(format!(
                "🔴 CRITICAL (must fix before merge):\n{}",
                items.join("\n")
            ));
        }

        // High: Concurrency and resource management
        let high_checks: Vec<&str> = self
            .concurrency_checks
            .iter()
            .chain(self.resource_checks.iter())
            .take(4)
            .map(String::as_str)
            .collect();
        if !high_checks.is_empty() {
            let items: Vec<String> = high_checks.iter().map(|s| format!("  - {}", s)).collect();
            sections.push(format!(
                "🟠 HIGH (should fix before merge):\n{}",
                items.join("\n")
            ));
        }

        // Medium: Quality, anti-patterns, performance
        let medium_checks: Vec<&str> = self
            .quality_checks
            .iter()
            .chain(self.anti_patterns.iter())
            .chain(self.performance_checks.iter())
            .take(5)
            .map(String::as_str)
            .collect();
        if !medium_checks.is_empty() {
            let items: Vec<String> = medium_checks.iter().map(|s| format!("  - {}", s)).collect();
            sections.push(format!("🟡 MEDIUM (should address):\n{}", items.join("\n")));
        }

        // Low: Testing, documentation, observability
        let low_checks: Vec<&str> = self
            .testing_checks
            .iter()
            .chain(self.documentation_checks.iter())
            .chain(self.observability_checks.iter())
            .take(4)
            .map(String::as_str)
            .collect();
        if !low_checks.is_empty() {
            let items: Vec<String> = low_checks.iter().map(|s| format!("  - {}", s)).collect();
            sections.push(format!("🟢 LOW (nice to have):\n{}", items.join("\n")));
        }

        sections.join("\n\n")
    }

    /// Get a brief summary for display
    pub(crate) fn summary(&self) -> String {
        format!(
            "{} quality checks, {} security checks, {} anti-patterns",
            self.quality_checks.len(),
            self.security_checks.len(),
            self.anti_patterns.len()
        )
    }

    /// Get a comprehensive count of all checks
    pub(crate) fn total_checks(&self) -> usize {
        self.quality_checks.len()
            + self.security_checks.len()
            + self.performance_checks.len()
            + self.testing_checks.len()
            + self.documentation_checks.len()
            + self.idioms.len()
            + self.anti_patterns.len()
            + self.concurrency_checks.len()
            + self.resource_checks.len()
            + self.observability_checks.len()
            + self.secrets_checks.len()
            + self.api_design_checks.len()
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
    fn test_check_severity_ordering() {
        // Critical should be less than (higher priority) than High, etc.
        assert!(CheckSeverity::Critical < CheckSeverity::High);
        assert!(CheckSeverity::High < CheckSeverity::Medium);
        assert!(CheckSeverity::Medium < CheckSeverity::Low);
        assert!(CheckSeverity::Low < CheckSeverity::Info);
    }

    #[test]
    fn test_check_severity_display() {
        assert_eq!(format!("{}", CheckSeverity::Critical), "CRITICAL");
        assert_eq!(format!("{}", CheckSeverity::High), "HIGH");
        assert_eq!(format!("{}", CheckSeverity::Medium), "MEDIUM");
        assert_eq!(format!("{}", CheckSeverity::Low), "LOW");
        assert_eq!(format!("{}", CheckSeverity::Info), "INFO");
    }

    #[test]
    fn test_severity_check_constructors() {
        let critical = SeverityCheck::critical("test");
        assert_eq!(critical.severity, CheckSeverity::Critical);
        assert_eq!(critical.check, "test");

        let high = SeverityCheck::high("high test");
        assert_eq!(high.severity, CheckSeverity::High);

        let medium = SeverityCheck::medium("medium test");
        assert_eq!(medium.severity, CheckSeverity::Medium);

        let low = SeverityCheck::low("low test");
        assert_eq!(low.severity, CheckSeverity::Low);
    }

    #[test]
    fn test_get_all_checks() {
        let guidelines = ReviewGuidelines::default();
        let all_checks = guidelines.get_all_checks();

        // Should have checks from all categories
        assert!(!all_checks.is_empty());

        // Security checks should be critical
        let critical_count = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Critical)
            .count();
        assert!(critical_count > 0);

        // Should have some medium severity checks
        let medium_count = all_checks
            .iter()
            .filter(|c| c.severity == CheckSeverity::Medium)
            .count();
        assert!(medium_count > 0);
    }

    #[test]
    fn test_format_for_prompt_with_priorities() {
        let guidelines = ReviewGuidelines::default();
        let formatted = guidelines.format_for_prompt_with_priorities();

        // Should contain priority indicators
        assert!(formatted.contains("CRITICAL"));
        assert!(formatted.contains("HIGH"));
        assert!(formatted.contains("MEDIUM"));
        assert!(formatted.contains("LOW"));
    }

    #[test]
    fn test_total_checks() {
        let guidelines = ReviewGuidelines::default();
        let total = guidelines.total_checks();

        // Should be the sum of all check categories
        let expected = guidelines.quality_checks.len()
            + guidelines.security_checks.len()
            + guidelines.performance_checks.len()
            + guidelines.testing_checks.len()
            + guidelines.documentation_checks.len()
            + guidelines.idioms.len()
            + guidelines.anti_patterns.len()
            + guidelines.concurrency_checks.len()
            + guidelines.resource_checks.len()
            + guidelines.observability_checks.len()
            + guidelines.secrets_checks.len()
            + guidelines.api_design_checks.len();

        assert_eq!(total, expected);
        assert!(total > 10); // Should have a reasonable number of checks
    }

    #[test]
    fn test_default_has_new_check_categories() {
        let guidelines = ReviewGuidelines::default();

        // New categories should have defaults
        assert!(!guidelines.concurrency_checks.is_empty());
        assert!(!guidelines.resource_checks.is_empty());
        assert!(!guidelines.observability_checks.is_empty());
        assert!(!guidelines.secrets_checks.is_empty());
        assert!(!guidelines.api_design_checks.is_empty());
    }

    // ============================================================================
    // Additional Language Guidelines Tests
    // ============================================================================

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
        assert!(guidelines
            .quality_checks
            .iter()
            .any(|c| c.contains("PSR")));
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

    // ============================================================================
    // Framework-specific Guidelines Tests
    // ============================================================================

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

    // ============================================================================
    // Edge Cases and Format Tests
    // ============================================================================

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
}
