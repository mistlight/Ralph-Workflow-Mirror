// Template rendering logic: variable substitution, conditional expansion, loops.

struct LiteralSegment {
    token: String,
    content: String,
}

struct LoopRenderLog {
    token: String,
    substituted: Vec<crate::prompts::SubstitutionEntry>,
    unsubstituted: Vec<String>,
}

impl Template {
    /// Process conditionals in the content based on variable values.
    ///
    /// Supports:
    /// - `{% if VARIABLE %}...{% endif %}` - show content if VARIABLE is truthy
    /// - `{% if !VARIABLE %}...{% endif %}` - show content if VARIABLE is falsy
    ///
    /// A variable is considered "truthy" if it exists and is non-empty.
    fn process_conditionals(content: &str, variables: &HashMap<&str, String>) -> String {
        let mut result = content.to_string();

        // Find all {% if ... %} blocks
        while let Some(start) = result.find("{% if ") {
            // Find the end of the if condition
            let if_end_start = start + 6; // "{% if " is 6 chars
            let if_end = if let Some(pos) = result[if_end_start..].find("%}") {
                if_end_start + pos + 2
            } else {
                // Unclosed if tag - skip it
                result = result[start + 1..].to_string();
                continue;
            };

            // Extract the condition
            let condition = result[if_end_start..if_end - 2].trim().to_string();

            // Find the matching {% endif %}
            let endif_start = if let Some(pos) = result[if_end..].find("{% endif %}") {
                if_end + pos
            } else {
                // Unclosed if block - skip it
                result = result[start + 1..].to_string();
                continue;
            };

            let endif_end = endif_start + 11; // "{% endif %}" is 11 chars

            // Extract the content inside the if block
            let block_content = result[if_end..endif_start].to_string();

            // Evaluate the condition
            let should_show = Self::evaluate_condition(&condition, variables);

            // Replace the entire if block with the content or empty string
            let replacement = if should_show {
                block_content
            } else {
                String::new()
            };
            result.replace_range(start..endif_end, &replacement);
        }

        result
    }

    /// Evaluate a conditional expression.
    ///
    /// Supports:
    /// - `VARIABLE` - true if variable exists and is non-empty
    /// - `!VARIABLE` - true if variable doesn't exist or is empty
    fn evaluate_condition(condition: &str, variables: &HashMap<&str, String>) -> bool {
        let condition = condition.trim();

        // Check for negation
        if let Some(rest) = condition.strip_prefix('!') {
            let var_name = rest.trim();
            let value = variables.get(var_name);
            return value.is_none_or(String::is_empty);
        }

        // Normal condition - check if variable exists and is non-empty
        let value = variables.get(condition);
        value.is_some_and(|v| !v.is_empty())
    }

    /// Process loops in the content based on variable values, returning substitution tracking.
    fn process_loops_with_log(
        content: &str,
        variables: &HashMap<&str, String>,
        literal_segments: &mut Vec<LiteralSegment>,
    ) -> (String, Vec<LoopRenderLog>) {
        let mut result = content.to_string();
        let mut loop_logs = Vec::new();

        // Find all {% for ... %} blocks
        while let Some(start) = result.find("{% for ") {
            // Find the end of the for condition
            let for_end_start = start + 7; // "{% for " is 7 chars
            let for_end = if let Some(pos) = result[for_end_start..].find("%}") {
                for_end_start + pos + 2
            } else {
                // Unclosed for tag - skip it
                result = result[start + 1..].to_string();
                continue;
            };

            // Parse "item in ITEMS"
            let condition = result[for_end_start..for_end - 2].trim();
            let parts: Vec<&str> = condition.split(" in ").collect();
            if parts.len() != 2 {
                // Invalid for syntax - skip it
                result = result[start + 1..].to_string();
                continue;
            }

            let loop_var = parts[0].trim().to_string();
            let list_var = parts[1].trim();

            // Find the matching {% endfor %}
            let endfor_start = if let Some(pos) = result[for_end..].find("{% endfor %}") {
                for_end + pos
            } else {
                // Unclosed for block - skip it
                result = result[start + 1..].to_string();
                continue;
            };

            let endfor_end = endfor_start + 12; // "{% endfor %}" is 12 chars

            // Extract the template inside the for block
            let block_template = result[for_end..endfor_start].to_string();

            // Get the list of values
            let items: Vec<String> = variables.get(list_var).map_or(Vec::new(), |v| {
                if v.is_empty() {
                    Vec::new()
                } else {
                    // Split by comma and trim each item
                    v.split(',').map(|s| s.trim().to_string()).collect()
                }
            });

            // Build the loop output
            let mut loop_output = String::new();
            let mut substituted = Vec::new();
            let mut unsubstituted = Vec::new();
            for item in items {
                // Create a temporary variable map with the loop variable
                let mut loop_vars: HashMap<&str, String> = variables.clone();
                loop_vars.insert(&loop_var, item);

                // Process conditionals first with loop variables
                let processed = Self::process_conditionals(&block_template, &loop_vars);

                // Then substitute variables (collect log for loop substitutions)
                let (processed, loop_substituted, loop_unsubstituted) =
                    Self::substitute_variables(&processed, &loop_vars);
                substituted.extend(loop_substituted);
                for name in loop_unsubstituted {
                    if !unsubstituted.contains(&name) {
                        unsubstituted.push(name);
                    }
                }
                loop_output.push_str(&processed);
            }

            // Replace the entire for block with the loop output
            let token = Self::next_literal_token(&result, &loop_output, literal_segments);
            literal_segments.push(LiteralSegment {
                token: token.clone(),
                content: loop_output,
            });
            result.replace_range(start..endfor_end, &token);

            loop_logs.push(LoopRenderLog {
                token,
                substituted,
                unsubstituted,
            });
        }

        (result, loop_logs)
    }

    /// Substitute variables in content (simple version without partials or conditionals).
    /// Returns `(result, substituted, unsubstituted)` where:
    /// - `substituted` is a list of `SubstitutionEntry` tracking how each var was resolved
    /// - `unsubstituted` is a list of variable names that had no value AND no default
    fn substitute_variables(
        content: &str,
        variables: &HashMap<&str, String>,
    ) -> (String, Vec<crate::prompts::SubstitutionEntry>, Vec<String>) {
        Self::substitute_variables_with_empty_policy(content, variables, true)
    }

    /// Substitute variables while allowing empty values without marking them missing.
    fn substitute_variables_allow_empty(
        content: &str,
        variables: &HashMap<&str, String>,
    ) -> (String, Vec<crate::prompts::SubstitutionEntry>, Vec<String>) {
        Self::substitute_variables_with_empty_policy(content, variables, false)
    }

    fn substitute_variables_with_empty_policy(
        content: &str,
        variables: &HashMap<&str, String>,
        empty_is_missing: bool,
    ) -> (String, Vec<crate::prompts::SubstitutionEntry>, Vec<String>) {
        use crate::prompts::{SubstitutionEntry, SubstitutionSource};

        let mut result = content.to_string();
        let mut substituted = Vec::new();
        let mut unsubstituted = Vec::new();

        // Find all {{...}} patterns
        let mut replacements = Vec::new();
        let mut i = 0;
        let bytes = content.as_bytes();
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                let start = i;
                i += 2;

                // Skip whitespace after {{
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }

                let name_start = i;

                // Find the closing }}
                while i < bytes.len()
                    && !(bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}')
                {
                    i += 1;
                }

                if i < bytes.len()
                    && bytes[i] == b'}'
                    && i + 1 < bytes.len()
                    && bytes[i + 1] == b'}'
                {
                    let end = i + 2;
                    let var_spec = &content[name_start..i];

                    // Check for partial reference {{> partial}} - skip it
                    if var_spec.trim().starts_with('>') {
                        i = end;
                        continue;
                    }

                    // Skip if variable name is empty or whitespace only
                    let trimmed_var = var_spec.trim();
                    if trimmed_var.is_empty() {
                        i = end;
                        continue;
                    }

                    // Check for default value syntax: {{VAR|default="value"}}
                    let (var_name, default_value) =
                        var_spec.find('|').map_or((trimmed_var, None), |pipe_pos| {
                            let name = var_spec[..pipe_pos].trim();
                            let rest = &var_spec[pipe_pos + 1..];
                            // Parse default="value"
                            rest.find('=').map_or((name, None), |eq_pos| {
                                let key = rest[..eq_pos].trim();
                                if key == "default" {
                                    let value = rest[eq_pos + 1..].trim();
                                    // Remove quotes if present (both single and double)
                                    let value = if (value.starts_with('"') && value.ends_with('"'))
                                        || (value.starts_with('\'') && value.ends_with('\''))
                                    {
                                        &value[1..value.len() - 1]
                                    } else {
                                        value
                                    };
                                    (name, Some(value.to_string()))
                                } else {
                                    (name, None)
                                }
                            })
                        });

                    // Look up the variable and track how it was resolved
                    let (replacement, should_replace, source) = if let Some(value) =
                        variables.get(var_name)
                    {
                        if !value.is_empty() {
                            // Value provided and non-empty
                            (value.clone(), true, Some(SubstitutionSource::Value))
                        } else if let Some(default) = &default_value {
                            // Value provided but empty, use default
                            (
                                default.clone(),
                                true,
                                Some(SubstitutionSource::EmptyWithDefault),
                            )
                        } else {
                            // Variable exists but is empty, and no default - treat as missing if configured
                            if empty_is_missing {
                                unsubstituted.push(var_name.to_string());
                            }
                            (String::new(), false, None)
                        }
                    } else if let Some(default) = &default_value {
                        (default.clone(), true, Some(SubstitutionSource::Default))
                    } else {
                        // No value AND no default - truly unsubstituted
                        unsubstituted.push(var_name.to_string());
                        (String::new(), false, None)
                    };

                    if should_replace {
                        if let Some(src) = source {
                            substituted.push(SubstitutionEntry {
                                name: var_name.to_string(),
                                source: src,
                            });
                        }
                        replacements.push((start, end, replacement));
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }

        // Apply replacements in reverse order to maintain correct positions
        for (start, end, replacement) in replacements.into_iter().rev() {
            result.replace_range(start..end, &replacement);
        }

        (result, substituted, unsubstituted)
    }

    fn next_literal_token(
        result: &str,
        content: &str,
        literal_segments: &[LiteralSegment],
    ) -> String {
        let mut index = literal_segments.len();
        loop {
            let token = format!("__RALPH_TEMPLATE_LITERAL_{index}__");
            if !result.contains(&token) && !content.contains(&token) {
                return token;
            }
            index += 1;
        }
    }

    fn restore_literal_segments(
        mut content: String,
        literal_segments: &[LiteralSegment],
    ) -> String {
        for segment in literal_segments.iter().rev() {
            content = content.replace(&segment.token, &segment.content);
        }
        content
    }

    /// Render the template with the provided variables.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    pub fn render(&self, variables: &HashMap<&str, String>) -> Result<String, TemplateError> {
        let mut literal_segments = Vec::new();
        // Process loops first (they may generate new variable references)
        let (mut result, loop_logs) = Self::process_loops_with_log(
            &self.content,
            variables,
            &mut literal_segments,
        );

        // Process conditionals
        result = Self::process_conditionals(&result, variables);

        // Substitute variables (with default values and substitution tracking)
        let (result_after_sub, _substituted, unsubstituted) =
            Self::substitute_variables_allow_empty(&result, variables);

        // Check for missing variables
        let mut missing = Vec::new();
        for loop_log in loop_logs {
            if result.contains(&loop_log.token) {
                for name in loop_log.unsubstituted {
                    if !missing.contains(&name) {
                        missing.push(name);
                    }
                }
            }
        }
        for name in unsubstituted {
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
        if let Some(first_missing) = missing.first() {
            return Err(TemplateError::MissingVariable(first_missing.clone()));
        }

        Ok(Self::restore_literal_segments(result_after_sub, &literal_segments))
    }

    /// Render the template with variables and partials support.
    ///
    /// Partials are processed recursively, with the same variables passed to each partial.
    /// Circular references are detected and reported with a clear error.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    pub fn render_with_partials(
        &self,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
    ) -> Result<String, TemplateError> {
        self.render_with_partials_recursive(variables, partials, &mut Vec::new())
    }

    /// Render the template with variables and partials, returning substitution log.
    ///
    /// This is the primary method for reducer-integrated rendering. It returns both
    /// the rendered content and a detailed log of all substitutions, enabling
    /// validation based on what was actually substituted rather than regex scanning.
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails.
    pub fn render_with_log(
        &self,
        template_name: &str,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
    ) -> Result<crate::prompts::RenderedTemplate, TemplateError> {
        self.render_with_log_recursive(template_name, variables, partials, &mut Vec::new())
    }

    /// Internal recursive rendering with circular reference detection.
    /// `visited` is a Vec that tracks the order of partials visited for proper error reporting.
    fn render_with_partials_recursive(
        &self,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
        visited: &mut Vec<String>,
    ) -> Result<String, TemplateError> {
        // First, extract and resolve all partials in this template
        let mut literal_segments = Vec::new();
        let mut result = self.content.clone();

        // Find all {{> partial}} references
        let partial_refs = Self::extract_partials(&result);

        // Process partials in reverse order to maintain correct positions when replacing
        for (full_match, partial_name) in partial_refs.into_iter().rev() {
            // Check for circular reference
            if visited.contains(&partial_name) {
                let mut chain = visited.clone();
                chain.push(partial_name);
                return Err(TemplateError::CircularReference(chain));
            }

            // Look up the partial content
            let partial_content = partials
                .get(&partial_name)
                .ok_or_else(|| TemplateError::PartialNotFound(partial_name.clone()))?;

            // Create a template from the partial and render it recursively
            let partial_template = Self::new(partial_content);
            visited.push(partial_name.clone());
            let rendered_partial =
                partial_template.render_with_partials_recursive(variables, partials, visited)?;
            visited.pop();

            // Replace the partial reference with rendered content
            let token = Self::next_literal_token(&result, &rendered_partial, &literal_segments);
            literal_segments.push(LiteralSegment {
                token: token.clone(),
                content: rendered_partial,
            });
            result = result.replace(&full_match, &token);
        }

        // Process loops (they may generate new variable references)
        let (loop_processed, loop_logs) = Self::process_loops_with_log(
            &result,
            variables,
            &mut literal_segments,
        );
        result = loop_processed;

        // Process conditionals
        result = Self::process_conditionals(&result, variables);

        // Now substitute variables in the result (using the new method that handles defaults)
        let (result_after_sub, _substituted, unsubstituted) =
            Self::substitute_variables_allow_empty(&result, variables);

        // Check for missing variables
        let mut missing = Vec::new();
        for loop_log in loop_logs {
            if result.contains(&loop_log.token) {
                for name in loop_log.unsubstituted {
                    if !missing.contains(&name) {
                        missing.push(name);
                    }
                }
            }
        }
        for name in unsubstituted {
            if !missing.contains(&name) {
                missing.push(name);
            }
        }
        if let Some(first_missing) = missing.first() {
            return Err(TemplateError::MissingVariable(first_missing.clone()));
        }

        Ok(Self::restore_literal_segments(
            result_after_sub,
            &literal_segments,
        ))
    }

    /// Internal recursive rendering with log tracking.
    fn render_with_log_recursive(
        &self,
        template_name: &str,
        variables: &HashMap<&str, String>,
        partials: &HashMap<String, String>,
        visited: &mut Vec<String>,
    ) -> Result<crate::prompts::RenderedTemplate, TemplateError> {
        use crate::prompts::{RenderedTemplate, SubstitutionLog};

        let mut log = SubstitutionLog {
            template_name: template_name.to_string(),
            substituted: Vec::new(),
            unsubstituted: Vec::new(),
        };
        let mut literal_segments = Vec::new();

        // Process partials (existing logic)
        let mut result = self.content.clone();
        let partial_refs = Self::extract_partials(&result);

        for (full_match, partial_name) in partial_refs.into_iter().rev() {
            if visited.contains(&partial_name) {
                let mut chain = visited.clone();
                chain.push(partial_name);
                return Err(TemplateError::CircularReference(chain));
            }

            let partial_content = partials
                .get(&partial_name)
                .ok_or_else(|| TemplateError::PartialNotFound(partial_name.clone()))?;

            let partial_template = Self::new(partial_content);
            visited.push(partial_name.clone());
            let rendered_partial = partial_template.render_with_log_recursive(
                template_name,
                variables,
                partials,
                visited,
            )?;
            visited.pop();

            let token = Self::next_literal_token(
                &result,
                &rendered_partial.content,
                &literal_segments,
            );
            literal_segments.push(LiteralSegment {
                token: token.clone(),
                content: rendered_partial.content,
            });
            result = result.replace(&full_match, &token);
            log.substituted.extend(rendered_partial.log.substituted);
            for name in rendered_partial.log.unsubstituted {
                if !log.unsubstituted.contains(&name) {
                    log.unsubstituted.push(name);
                }
            }
        }

        // Process loops
        let (loop_processed, loop_logs) =
            Self::process_loops_with_log(
                &result,
                variables,
                &mut literal_segments,
            );
        result = loop_processed;

        // Process conditionals
        result = Self::process_conditionals(&result, variables);

        for loop_log in loop_logs {
            if result.contains(&loop_log.token) {
                log.substituted.extend(loop_log.substituted);
                for name in loop_log.unsubstituted {
                    if !log.unsubstituted.contains(&name) {
                        log.unsubstituted.push(name);
                    }
                }
            }
        }

        // Substitute variables WITH log tracking
        let (result_after_sub, substituted, unsubstituted) =
            Self::substitute_variables(&result, variables);

        log.substituted.extend(substituted);
        for name in unsubstituted {
            if !log.unsubstituted.contains(&name) {
                log.unsubstituted.push(name);
            }
        }

        Ok(RenderedTemplate {
            content: Self::restore_literal_segments(result_after_sub, &literal_segments),
            log,
        })
    }
}
