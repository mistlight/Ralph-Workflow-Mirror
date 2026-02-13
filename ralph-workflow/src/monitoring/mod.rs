//! Production monitoring and metrics.
//!
//! This module provides observability features for production deployments.
//! All monitoring features are gated behind the `monitoring` feature flag.

#[cfg(feature = "monitoring")]
pub mod memory_metrics;
