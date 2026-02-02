/// Parse a u32 environment variable with validation.
pub(super) fn parse_env_u32(name: &str, warnings: &mut Vec<String>, max: u32) -> Option<u32> {
    let raw = std::env::var(name).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.parse::<u32>() {
        Ok(n) if n <= max => Some(n),
        Ok(n) => {
            warnings.push(format!(
                "Env var {name}={n} is too large; clamping to {max}."
            ));
            Some(max)
        }
        Err(_) => {
            warnings.push(format!(
                "Env var {name}='{trimmed}' is not a valid number; ignoring."
            ));
            None
        }
    }
}

/// Parse a u8 environment variable with validation.
pub(super) fn parse_env_u8(name: &str, warnings: &mut Vec<String>, max: u8) -> Option<u8> {
    let raw = std::env::var(name).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.parse::<u8>() {
        Ok(n) if n <= max => Some(n),
        Ok(n) => {
            warnings.push(format!(
                "Env var {name}={n} is out of range; clamping to {max}."
            ));
            Some(max)
        }
        Err(_) => {
            warnings.push(format!(
                "Env var {name}='{trimmed}' is not a valid number; ignoring."
            ));
            None
        }
    }
}
