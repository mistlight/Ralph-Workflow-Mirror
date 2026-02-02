// NOTE: split from reducer/event.rs to keep the facade small.
// Constructors are further split by event category to keep files under 500 lines.
use super::*;

// Include constructor implementations split by category
include!("constructors_lifecycle.rs");
include!("constructors_development.rs");
include!("constructors_review.rs");
include!("constructors_agent.rs");
include!("constructors_commit.rs");
