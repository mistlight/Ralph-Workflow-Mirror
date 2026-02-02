/// Format a validation error for display.
fn format_error(error: &crate::prompts::ValidationError) -> String {
    match error {
        crate::prompts::ValidationError::UnclosedConditional { line } => {
            format!("unclosed conditional block on line {line}")
        }
        crate::prompts::ValidationError::UnclosedLoop { line } => {
            format!("unclosed loop block on line {line}")
        }
        crate::prompts::ValidationError::InvalidConditional { line, syntax } => {
            format!("invalid conditional syntax on line {line}: '{syntax}'")
        }
        crate::prompts::ValidationError::InvalidLoop { line, syntax } => {
            format!("invalid loop syntax on line {line}: '{syntax}'")
        }
        crate::prompts::ValidationError::UnclosedComment { line } => {
            format!("unclosed comment on line {line}")
        }
        crate::prompts::ValidationError::PartialNotFound { name } => {
            format!("partial not found: '{name}'")
        }
    }
}

/// Format a validation warning for display.
fn format_warning(warning: &crate::prompts::ValidationWarning) -> String {
    match warning {
        crate::prompts::ValidationWarning::VariableMayError { name } => {
            format!("variable '{name}' may cause error if not provided")
        }
    }
}
