// Health module tests.

#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/parser_health.rs");
    include!("tests/streaming_quality_metrics.rs");
}
