//! Reviewer prompts.
//!
//! Prompts for reviewer agent actions including review, comprehensive review,
//! security-focused review, and incremental review.

mod guided;
mod unguided;

pub use guided::{
    prompt_comprehensive_review, prompt_reviewer_review_with_guidelines,
    prompt_security_focused_review,
};
pub use unguided::{
    prompt_detailed_review_without_guidelines, prompt_incremental_review_with_diff,
    prompt_reviewer_review, prompt_universal_review,
};

#[cfg(test)]
mod tests;
