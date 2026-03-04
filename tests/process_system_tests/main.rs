// DO NOT CHANGE THESE CLIPPY SETTINGS, YOU MUST REFACTOR INSTEAD, EVEN IF IT TAKES YOU 100 YEARS
// Note: unsafe_code is not denied in test code because tests may require unsafe blocks for
// low-level testing (e.g., signal handling, timezone manipulation). All unsafe blocks must
// have proper safety documentation explaining why they are safe.
//
// Note: clippy::cargo is not enabled because it flags transitive dependency version conflicts
// (e.g., bitflags 1.3.2 from inotify vs 2.10.0 from other crates) which are ecosystem-level
// issues outside our control and don't reflect code quality problems.
#![deny(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
//! Process-level system tests: agent binary discovery.
//!
//! These tests require real OS processes, real file permissions, or real PATH
//! discovery, but do NOT use libgit2. They are intentionally separated from the
//! `git2-system-tests` binary so that the libgit2 global
//! reference-counter constraint does not force serialization here.
//!
//! # Parallelism
//!
//! Tests in this binary run in parallel by default (standard Rust behavior).
//! Each test must create its own isolated `TempDir` and spawned processes.
//! If a test modifies process-global state (e.g. PATH), it must hold the
//! module-local `ENV_LOCK` Mutex for the duration; it must NOT use `#[serial]`.
//!
//! # Not in CI
//!
//! System tests are run manually only. See `docs/agents/testing-guide.md`.

mod agents;
mod deduplication;
