//! Unit tests for XSD plan validation.
//!
//! Tests are organized by category to keep files manageable and enable parallel test execution.
//! Each module tests a specific aspect of plan validation.

use super::*;

// Test modules organized by category
mod content_elements;
mod edge_cases;
mod file_validation;
mod minimal_valid;
mod risk_verification;
mod scope_validation;
mod step_validation;
