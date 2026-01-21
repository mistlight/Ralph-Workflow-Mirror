//! OpenCode API catalog module.
//!
//! This module handles fetching, caching, and querying the OpenCode model catalog
//! from https://models.dev/api.json. The catalog contains available providers and models
//! that OpenCode supports, enabling dynamic agent configuration.
//!
//! # Module Structure
//!
//! - [`types`] - API catalog data structures
//! - [`cache`] - File-based caching with TTL
//! - [`fetch`] - HTTP fetching logic

mod cache;
mod fetch;
mod types;

pub use cache::load_api_catalog;
pub use types::{ApiCatalog, Model, Provider};

/// OpenCode API endpoint for model catalog.
pub const API_URL: &str = "https://models.dev/api.json";

/// Default cache TTL in seconds (24 hours).
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 24 * 60 * 60;

/// Environment variable for customizing cache TTL.
pub const CACHE_TTL_ENV_VAR: &str = "RALPH_OPENCODE_CACHE_TTL_SECONDS";
