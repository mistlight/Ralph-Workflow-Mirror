//! Reviewer prompts.
//!
//! Prompts for reviewer agent actions including review, comprehensive review,
//! security-focused review, and incremental review.

mod guided;
mod unguided;

pub use guided::{
    prompt_comprehensive_review_with_diff_with_context,
    prompt_reviewer_review_with_guidelines_and_diff_with_context,
    prompt_security_focused_review_with_diff_with_context,
};

// Re-export non-context variants for test compatibility
#[cfg(test)]
pub use guided::prompt_reviewer_review_with_guidelines_and_diff;
#[cfg(test)]
pub use unguided::prompt_detailed_review_without_guidelines_with_diff;
