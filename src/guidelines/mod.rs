//! Language-Specific Review Guidelines Module
//!
//! Provides tailored code review guidance based on the detected project stack.
//! These guidelines are incorporated into review prompts to help agents focus
//! on language-specific best practices, common pitfalls, and security concerns.

#![deny(unsafe_code)]

mod base;
mod functional;
mod go;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust;
mod stack;
mod systems;

// Re-export public types.
pub(crate) use base::{CheckSeverity, ReviewGuidelines};

#[cfg(test)]
mod tests;

