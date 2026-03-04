//! System tests for streaming output deduplication.
//!
//! This module previously contained parser-focused tests that relied on a real
//! fixture file. Those tests have been removed from system tests to keep this
//! suite boundary-only. The behavior coverage for deduplication lives in
//! `tests/integration_tests/deduplication/`.
