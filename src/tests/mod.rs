//! Tests for Legend v2 strict composable blocks.
//!
//! ## Test Organization
//!
//! - `common`: Shared types, context, errors, and step implementations
//! - `basic`: Basic execution tests (success paths)
//! - `compensation`: Rollback and compensation tests
//! - `retry`: Retry policy tests
//! - `pause`: Pause/resume and serialization tests
//!
//! ## Test Programs
//!
//! All tests use a "Math" domain with these steps:
//! - `Add`: Computes `a + b`, stores in a named register
//! - `Sub`: Computes `a - b`, stores in a named register
//! - `Halt`: Clears rollback log (signals completion)
//! - `Flaky`: Fails N times before succeeding (for retry tests)

mod common;

mod basic;
mod compensation;
mod pause;
mod retry;
