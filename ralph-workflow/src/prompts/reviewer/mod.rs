//! Reviewer prompts.
//!
//! Prompts for reviewer agent actions including review, comprehensive review,
//! security-focused review, and incremental review.

mod guided;
mod unguided;

pub use guided::{
    prompt_comprehensive_review_with_diff, prompt_reviewer_review_with_guidelines_and_diff,
    prompt_security_focused_review_with_diff,
};
pub use unguided::{
    prompt_detailed_review_without_guidelines_with_diff, prompt_incremental_review_with_diff,
    prompt_universal_review_with_diff,
};

#[cfg(test)]
mod tests;
