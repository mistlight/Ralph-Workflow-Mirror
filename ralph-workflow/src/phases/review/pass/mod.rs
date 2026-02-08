//! Review and fix pass orchestration.
//!
//! This module implements the review-fix cycle that validates code changes and
//! addresses identified issues. It provides two main pass types:
//!
//! - **Review Pass**: Validates code changes and extracts issues to ISSUES.md
//! - **Fix Pass**: Applies fixes for identified issues and validates results
//!
//! ## Architecture
//!
//! Both passes follow a similar pattern:
//! 1. Generate prompt (with checkpoint replay support)
//! 2. Invoke agent with appropriate configuration
//! 3. Extract and validate XML output
//! 4. Update execution history with results
//!
//! ## Usage
//!
//! ```rust
//! // Run a review pass
//! let result = run_review_pass(ctx, j, "review", &review_prompt, Some("reviewer-agent"))?;
//! if result.issues_found {
//!     // Run a fix pass to address issues
//!     let fix_result = run_fix_pass(ctx, j, context_level, None, Some("reviewer-agent"))?;
//! }
//! ```
//!
//! ## See Also
//!
//! - `phases::review::validation` - Post-flight validation logic
//! - `phases::review::xml_processing` - XML extraction and validation

mod fix;
mod helpers;
mod review;

pub use fix::run_fix_pass;
pub use review::run_review_pass;
