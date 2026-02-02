//! Idle timeout detection for agent subprocess execution.
//!
//! This module provides infrastructure to detect when an agent subprocess
//! has stopped producing output, indicating it may be stuck (e.g., waiting
//! for user input in unattended mode).
//!
//! # Design
//!
//! The idle timeout system uses a shared atomic timestamp that gets updated
//! whenever data is read from the subprocess stdout OR stderr. A monitor thread
//! periodically checks this timestamp and can kill the subprocess if
//! no output has been received for longer than the configured timeout.
//!
//! Both stdout and stderr activity are tracked because some agents (e.g., opencode
//! with `--print-logs`) output verbose progress information to stderr while
//! processing, and only produce stdout when complete. Without tracking stderr,
//! such agents would be incorrectly killed as idle.
//!
//! # Timeout Value
//!
//! The default timeout is 5 minutes (300 seconds), which is:
//! - Long enough for complex tool operations and LLM reasoning
//! - Short enough to detect truly stuck agents
//! - Aligned with typical CI/CD step timeouts

include!("idle_timeout/part1.rs");
include!("idle_timeout/part2.rs");
include!("idle_timeout/part3.rs");
