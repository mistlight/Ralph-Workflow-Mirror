/// Quick preset mode flags.
#[derive(Parser, Debug, Default)]
pub struct QuickPresets {
    /// Quick mode: 1 developer iteration, 1 review pass (fast turnaround)
    #[arg(
        long,
        short = 'Q',
        help = "Quick mode: 1 dev iteration + 1 review (for rapid prototyping)"
    )]
    pub quick: bool,

    /// Rapid mode: 2 developer iterations, 1 review pass (between quick and standard)
    #[arg(
        long,
        short = 'U',
        help = "Rapid mode: 2 dev iterations + 1 review (fast but more thorough than quick)"
    )]
    pub rapid: bool,

    /// Long mode: 15 developer iterations, 10 review passes (for thorough development)
    #[arg(
        long,
        short = 'L',
        help = "Long mode: 15 dev iterations + 10 reviews (for thorough development)"
    )]
    pub long: bool,
}

/// Standard preset mode flags.
#[derive(Parser, Debug, Default)]
pub struct StandardPresets {
    /// Standard mode: 5 developer iterations, 2 review passes (default workflow)
    #[arg(
        long,
        short = 'S',
        help = "Standard mode: 5 dev iterations + 2 reviews (default workflow)"
    )]
    pub standard: bool,

    /// Thorough mode: 10 developer iterations, 5 review passes (balanced but more than default)
    #[arg(
        long,
        short = 'T',
        help = "Thorough mode: 10 dev iterations + 5 reviews (balanced but thorough)"
    )]
    pub thorough: bool,
}
