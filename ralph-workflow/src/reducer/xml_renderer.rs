//! Semantic XML renderers - DEPRECATED, use `rendering::xml` instead.
//!
//! This module re-exports from `rendering::xml` for backward compatibility.
//! New code should import directly from `crate::rendering::xml`.
//!
//! # Migration
//!
//! Replace:
//! ```ignore
//! use crate::reducer::xml_renderer::render_xml;
//! ```
//!
//! With:
//! ```ignore
//! use crate::rendering::xml::render_xml;
//! ```

#[deprecated(
    since = "0.9.0",
    note = "Use crate::rendering::xml::render_xml instead"
)]
pub use crate::rendering::xml::render_xml;
