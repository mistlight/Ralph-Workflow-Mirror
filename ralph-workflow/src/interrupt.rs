//! Interrupt signal handling for graceful checkpoint save.
//!
//! This module provides signal handling for the Ralph pipeline, ensuring
//! clean shutdown when the user interrupts with Ctrl+C.

/// Set up the interrupt handler for graceful shutdown.
///
/// This function registers a SIGINT handler that will clean up
/// generated files and exit gracefully. Call this early in main().
///
/// # Note
///
/// Currently this handler only performs cleanup. Future enhancements
/// will add checkpoint saving on interrupt for seamless resume.
pub fn setup_interrupt_handler() {
    ctrlc::set_handler(|| {
        eprintln!("\n✋ Interrupt received! Cleaning up...");
        crate::git_helpers::cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    })
    .ok(); // Ignore errors if handler can't be set
}
