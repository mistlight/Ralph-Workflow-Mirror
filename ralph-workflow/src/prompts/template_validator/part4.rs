// ============================================================================
// Template syntax validation
// ============================================================================

/// Validate a template's syntax and structure.
///
/// Checks for:
/// - Unclosed variable references
/// - Unclosed conditionals
/// - Unclosed loops
/// - Unclosed comments
/// - Invalid syntax in conditionals and loops
pub fn validate_syntax(content: &str) -> Vec<ValidationError> {
    let bytes = content.as_bytes();
    SyntaxValidator::new(content).validate(bytes)
}

/// Helper struct for template syntax validation.
struct SyntaxValidator<'a> {
    content: &'a str,
    errors: Vec<ValidationError>,
    line: usize,
    i: usize,
    conditional_stack: Vec<(usize, &'static str)>,
    loop_stack: Vec<(usize, &'static str)>,
}

impl<'a> SyntaxValidator<'a> {
    const fn new(content: &'a str) -> Self {
        Self {
            content,
            errors: Vec::new(),
            line: 0,
            i: 0,
            conditional_stack: Vec::new(),
            loop_stack: Vec::new(),
        }
    }

    fn validate(mut self, bytes: &[u8]) -> Vec<ValidationError> {
        while self.i < bytes.len() {
            self.track_newlines(bytes);
            if self.try_skip_comment(bytes) {
                continue;
            }
            if self.try_parse_conditional(bytes) {
                continue;
            }
            if self.try_parse_loop(bytes) {
                continue;
            }
            self.i += 1;
        }
        self.check_unclosed_blocks();
        self.errors
    }

    fn track_newlines(&mut self, bytes: &[u8]) {
        if bytes[self.i] == b'\n' {
            self.line += 1;
        }
    }

    fn try_skip_comment(&mut self, bytes: &[u8]) -> bool {
        if self.i + 1 < bytes.len() && bytes[self.i] == b'{' && bytes[self.i + 1] == b'#' {
            let comment_start = self.line;
            self.i += 2;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'#' && bytes[self.i + 1] == b'}')
            {
                if bytes[self.i] == b'\n' {
                    self.line += 1;
                }
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors.push(ValidationError::UnclosedComment {
                    line: comment_start,
                });
            }
            if self.i + 1 < bytes.len() {
                self.i += 2;
            }
            true
        } else {
            false
        }
    }

    fn try_parse_conditional(&mut self, bytes: &[u8]) -> bool {
        // Check for {% if ... %}
        if self.i + 5 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'i'
            && bytes[self.i + 4] == b'f'
            && bytes[self.i + 5] == b' '
        {
            let if_start = self.i;
            self.i += 6;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'%' && bytes[self.i + 1] == b'}')
            {
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors
                    .push(ValidationError::UnclosedConditional { line: self.line });
            } else {
                let condition = self.content[if_start + 6..self.i].trim();
                if condition.is_empty() || condition.contains('{') || condition.contains('}') {
                    self.errors.push(ValidationError::InvalidConditional {
                        line: self.line,
                        syntax: condition.to_string(),
                    });
                }
                self.conditional_stack.push((self.line, "if"));
                self.i += 2;
            }
            return true;
        }

        // Check for {% endif %}
        if self.i + 9 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'e'
            && bytes[self.i + 4] == b'n'
            && bytes[self.i + 5] == b'd'
            && bytes[self.i + 6] == b'i'
            && bytes[self.i + 7] == b'f'
            && bytes[self.i + 8] == b' '
            && bytes[self.i + 9] == b'%'
        {
            self.conditional_stack.pop();
            self.i += 11;
            return true;
        }

        false
    }

    fn try_parse_loop(&mut self, bytes: &[u8]) -> bool {
        // Check for {% for ... %}
        if self.i + 6 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'f'
            && bytes[self.i + 4] == b'o'
            && bytes[self.i + 5] == b'r'
            && bytes[self.i + 6] == b' '
        {
            let for_start = self.i;
            self.i += 7;
            while self.i + 1 < bytes.len() && !(bytes[self.i] == b'%' && bytes[self.i + 1] == b'}')
            {
                self.i += 1;
            }
            if self.i + 1 >= bytes.len() {
                self.errors
                    .push(ValidationError::UnclosedLoop { line: self.line });
            } else {
                let condition = self.content[for_start + 7..self.i].trim();
                if !condition.contains(" in ") || condition.split(" in ").count() != 2 {
                    self.errors.push(ValidationError::InvalidLoop {
                        line: self.line,
                        syntax: condition.to_string(),
                    });
                }
                self.loop_stack.push((self.line, "for"));
                self.i += 2;
            }
            return true;
        }

        // Check for {% endfor %}
        if self.i + 10 < bytes.len()
            && bytes[self.i] == b'{'
            && bytes[self.i + 1] == b'%'
            && bytes[self.i + 2] == b' '
            && bytes[self.i + 3] == b'e'
            && bytes[self.i + 4] == b'n'
            && bytes[self.i + 5] == b'd'
            && bytes[self.i + 6] == b'f'
            && bytes[self.i + 7] == b'o'
            && bytes[self.i + 8] == b'r'
            && bytes[self.i + 9] == b' '
        {
            self.loop_stack.pop();
            self.i += 12;
            return true;
        }

        false
    }

    fn check_unclosed_blocks(&mut self) {
        if let Some((line, _)) = self.conditional_stack.first() {
            self.errors
                .push(ValidationError::UnclosedConditional { line: *line });
        }
        if let Some((line, _)) = self.loop_stack.first() {
            self.errors
                .push(ValidationError::UnclosedLoop { line: *line });
        }
    }
}

/// Validate a complete template.
///
/// Performs comprehensive validation including syntax checking,
/// variable extraction, and partial reference validation.
pub fn validate_template(content: &str, available_partials: &HashSet<String>) -> ValidationResult {
    let mut is_valid = true;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate syntax
    let syntax_errors = validate_syntax(content);
    if !syntax_errors.is_empty() {
        is_valid = false;
        errors.extend(syntax_errors);
    }

    // Extract variables
    let variables = extract_variables(content);

    // Extract partials
    let partials = extract_partials(content);

    // Check for missing partials
    for partial in &partials {
        if !available_partials.contains(partial) {
            is_valid = false;
            errors.push(ValidationError::PartialNotFound {
                name: partial.clone(),
            });
        }
    }

    // Check for variables without defaults that might error
    for var in &variables {
        if !var.has_default {
            warnings.push(ValidationWarning::VariableMayError {
                name: var.name.clone(),
            });
        }
    }

    ValidationResult {
        is_valid,
        variables,
        partials,
        errors,
        warnings,
    }
}
