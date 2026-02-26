//! Reviewer prompts.
//!
//! This module previously contained deprecated reviewer prompt templates.
//! All reviewer functionality now uses `review_xml.txt` template via
//! `prompt_review_xml_with_context()` in `src/prompts/review.rs`.
//!
//! The following templates and code have been removed as they were never
//! used in production:
//! - guided.rs (`prompt_reviewer_review_with_guidelines_and_diff`)
//! - unguided.rs (`prompt_detailed_review_without_guidelines_with_diff`)
//! - `templates/standard_review.txt`
//! - `templates/comprehensive_review.txt`
//! - `templates/security_review.txt`
//! - `templates/universal_review.txt`
//! - All *_minimal and *_normal variants
