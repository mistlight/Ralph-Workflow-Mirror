//! Environment variable parsing helpers.
//!
//! The unified config loader (`crate::config::loader`) owns the full
//! configuration-loading flow; this module keeps only shared helpers.

/// Parse a boolean from an environment variable value.
///
/// Accepts common truthy and falsy values:
/// - Truthy: "1", "true", "yes", "y", "on"
/// - Falsy: "0", "false", "no", "n", "off"
///
/// # Arguments
///
/// * `value` - The string value to parse
///
/// # Returns
///
/// Returns `Some(true)` for truthy values, `Some(false)` for falsy values,
/// and `None` for empty or unrecognized values.
#[must_use]
pub fn parse_env_bool(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_bool() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool(" TRUE "), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));
        assert_eq!(parse_env_bool("yes"), Some(true));

        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool(" FALSE "), Some(false));
        assert_eq!(parse_env_bool("off"), Some(false));
        assert_eq!(parse_env_bool("no"), Some(false));

        assert_eq!(parse_env_bool(""), None);
        assert_eq!(parse_env_bool("maybe"), None);
    }
}
