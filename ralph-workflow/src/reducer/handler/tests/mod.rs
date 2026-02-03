// Split from the legacy monolithic reducer/handler.rs test module.
// Individual test modules will be added here as the handler implementation is
// decomposed into single-task effects.

mod commit_handler;
mod development_outcome;
mod development_prompt;
mod extract_missing;
mod fix_outcome;
mod invoke_prompt;
mod planning_markdown;
mod planning_prompt;
mod review_prompt;
mod review_validation;
mod stale_xml_cleanup;
