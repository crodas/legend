//! Common types and step implementations for tests.
//!
//! This module contains:
//! - `MathContext`: The shared context for all math operations
//! - `MathError`: Error types for math operations
//! - Step implementations: `Add`, `Sub`, `Halt`, `Flaky`
//! - Program definitions using the `legend!` macro

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{legend, CompensationOutcome, RetryPolicy, Step, StepOutcome};

// ============================================================================
// Error Type
// ============================================================================

/// Errors that can occur during Math program execution.
#[derive(thiserror::Error, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MathError {
    /// Arithmetic overflow during addition.
    #[error("Overflow")]
    Overflow,

    /// Arithmetic underflow during subtraction.
    #[error("Underflow")]
    Underflow,

    /// Transient error that may succeed on retry.
    #[error("Transient")]
    Transient,
}

// ============================================================================
// Context Type
// ============================================================================

/// The context/state for Math programs.
///
/// Stores computed values in named registers and tracks state for rollback.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct MathContext {
    /// Named registers storing computed values.
    pub r: HashMap<Arc<str>, u8>,

    /// Tracks previous values for rollback.
    /// Key: register name, Value: previous value (None if was empty).
    pub to_rollback: HashMap<Arc<str>, Option<u8>>,

    /// If Some(n), pause after n operations.
    /// Decrements on each operation.
    pub pause_after: Option<usize>,

    /// Number of times Flaky should fail before succeeding.
    /// Decrements on each failure.
    pub fail_count: usize,
}

// ============================================================================
// Step Implementations
// ============================================================================

/// Add step: computes `a + b` and stores in a named register.
///
/// Tracks the previous value for rollback support.
/// Respects `pause_after` for testing pause/resume.
pub struct Add;

#[async_trait::async_trait]
impl Step<MathContext, MathError> for Add {
    type Input = (Arc<str>, u8, u8);

    async fn execute(ctx: &mut MathContext, input: &Self::Input) -> Result<StepOutcome, MathError> {
        let (name, a, b) = input;

        // Perform addition with overflow check
        let x = (*a).checked_add(*b).ok_or(MathError::Overflow)?;

        // Store result and track for rollback
        let previous = ctx.r.insert(name.clone(), x);
        if !ctx.to_rollback.contains_key(name) {
            ctx.to_rollback.insert(name.clone(), previous);
        }

        // Check if we should pause (for testing)
        if let Some(ref mut count) = ctx.pause_after {
            if *count == 0 {
                return Ok(StepOutcome::Pause);
            }
            *count -= 1;
        }

        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut MathContext,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, MathError> {
        let (name, _, _) = input;

        // Restore previous value
        match ctx.to_rollback.remove(name) {
            Some(Some(prev)) => {
                ctx.r.insert(name.clone(), prev);
            }
            Some(None) => {
                ctx.r.remove(name);
            }
            None => {}
        }

        Ok(CompensationOutcome::Completed)
    }
}

/// Sub step: computes `a - b` and stores in a named register.
///
/// Tracks the previous value for rollback support.
pub struct Sub;

#[async_trait::async_trait]
impl Step<MathContext, MathError> for Sub {
    type Input = (Arc<str>, u8, u8);

    async fn execute(ctx: &mut MathContext, input: &Self::Input) -> Result<StepOutcome, MathError> {
        let (name, a, b) = input;

        // Perform subtraction with underflow check
        let x = (*a).checked_sub(*b).ok_or(MathError::Underflow)?;

        // Store result and track for rollback
        let previous = ctx.r.insert(name.clone(), x);
        if !ctx.to_rollback.contains_key(name) {
            ctx.to_rollback.insert(name.clone(), previous);
        }

        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut MathContext,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, MathError> {
        let (name, _, _) = input;

        // Restore previous value
        match ctx.to_rollback.remove(name) {
            Some(Some(prev)) => {
                ctx.r.insert(name.clone(), prev);
            }
            Some(None) => {
                ctx.r.remove(name);
            }
            None => {}
        }

        Ok(CompensationOutcome::Completed)
    }
}

/// Halt step: clears the rollback log.
///
/// Signals that all operations completed successfully.
/// Typically used as the last step in a saga.
pub struct Halt;

#[async_trait::async_trait]
impl Step<MathContext, MathError> for Halt {
    type Input = ();

    async fn execute(
        ctx: &mut MathContext,
        _input: &Self::Input,
    ) -> Result<StepOutcome, MathError> {
        ctx.to_rollback.clear();
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        _ctx: &mut MathContext,
        _input: &Self::Input,
    ) -> Result<CompensationOutcome, MathError> {
        Ok(CompensationOutcome::Completed)
    }
}

/// Flaky step: fails until `fail_count` reaches 0.
///
/// Used for testing retry functionality.
/// Implements a retry policy of up to 3 attempts.
pub struct Flaky;

#[async_trait::async_trait]
impl Step<MathContext, MathError> for Flaky {
    type Input = Arc<str>;

    async fn execute(ctx: &mut MathContext, input: &Self::Input) -> Result<StepOutcome, MathError> {
        if ctx.fail_count > 0 {
            ctx.fail_count -= 1;
            return Err(MathError::Transient);
        }

        // Success: store a marker value
        ctx.r.insert(input.clone(), 1);
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut MathContext,
        input: &Self::Input,
    ) -> Result<CompensationOutcome, MathError> {
        ctx.r.remove(input);
        Ok(CompensationOutcome::Completed)
    }

    fn retry_policy() -> RetryPolicy {
        RetryPolicy::retries(3)
    }
}

// ============================================================================
// Program Definitions
// ============================================================================

// A math program with add, sub, and halt steps.
legend! {
    Math<MathContext, MathError> {
        add: Add,
        sub: Sub,
        halt: Halt,
    }
}

// A math program with a flaky step for retry testing.
legend! {
    MathWithFlaky<MathContext, MathError> {
        add: Add,
        flaky: Flaky,
        halt: Halt,
    }
}

// Single-step program with just Add.
legend! {
    SingleAdd<MathContext, MathError> {
        add: Add,
    }
}

// Single-step program with just Halt.
legend! {
    SingleHalt<MathContext, MathError> {
        halt: Halt,
    }
}
