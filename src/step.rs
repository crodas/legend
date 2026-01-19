//! Step trait and related types for the strict saga VM.
//!
//! A `Step` is an atomic unit of work with execute and compensate actions.
//! Steps are composed into blocks and programs using HList-based sequences.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Outcome of successful step execution.
///
/// Used with `Result<StepOutcome, E>` to enable `?` operator for error propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepOutcome {
    /// Continue to the next step.
    Continue,
    /// Pause execution (can be resumed later).
    Pause,
}

/// Outcome of successful compensation.
///
/// Used with `Result<CompensationOutcome, E>` to enable `?` operator for error propagation.
/// The error case represents a critical failure requiring manual intervention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompensationOutcome {
    /// Compensation completed successfully.
    Completed,
    /// Compensation needs to pause (can be resumed).
    Pause,
}

/// Retry policy for step execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RetryPolicy {
    /// Never retry - compensate immediately on failure.
    #[default]
    NoRetry,
    /// Retry up to max_attempts times with optional backoff.
    Retry {
        /// Maximum number of retry attempts (not counting initial attempt).
        max_attempts: u8,
        /// Backoff between retries in milliseconds.
        backoff_ms: u64,
    },
}

impl RetryPolicy {
    /// Create a retry policy with the given max attempts and no backoff.
    pub const fn retries(max_attempts: u8) -> Self {
        Self::Retry {
            max_attempts,
            backoff_ms: 0,
        }
    }

    /// Create a retry policy with the given max attempts and backoff.
    pub const fn retries_with_backoff(max_attempts: u8, backoff_ms: u64) -> Self {
        Self::Retry {
            max_attempts,
            backoff_ms,
        }
    }

    /// Check if retries are exhausted for the given attempt count.
    pub fn should_retry(&self, attempt: u8) -> bool {
        match self {
            Self::NoRetry => false,
            Self::Retry { max_attempts, .. } => attempt < *max_attempts,
        }
    }

    /// Get the backoff duration in milliseconds.
    pub fn backoff_ms(&self) -> u64 {
        match self {
            Self::NoRetry => 0,
            Self::Retry { backoff_ms, .. } => *backoff_ms,
        }
    }
}

/// A step that can be executed and compensated.
///
/// This is the core trait for saga steps. Each step has:
/// - An input type that is serializable
/// - An execute method for the forward action
/// - A compensate method for the rollback action
/// - A retry policy for handling transient failures
///
/// # Type Parameters
/// - `Ctx`: The context type shared across all steps
/// - `Err`: The error type for this step
#[async_trait::async_trait]
pub trait Step<Ctx, Err>: Send + Sync + 'static
where
    Ctx: Send + Sync,
    Err: Send + Sync,
{
    /// The input data for this step.
    type Input: Serialize + DeserializeOwned + Send + Sync + Clone + 'static;

    /// Execute the forward action.
    ///
    /// This method performs the main work of the step. Results should be
    /// stored in the context for use by subsequent steps or compensation.
    ///
    /// Returns `Ok(StepOutcome::Continue)` to proceed, `Ok(StepOutcome::Pause)` to suspend,
    /// or `Err(e)` to trigger retry/compensation.
    async fn execute(ctx: &mut Ctx, input: &Self::Input) -> Result<StepOutcome, Err>;

    /// Compensate (undo) the forward action.
    ///
    /// This method is called during rollback to undo the effects of execute.
    /// Returns `Ok(CompensationOutcome::Completed)` on success, `Ok(CompensationOutcome::Pause)` to suspend,
    /// or `Err(e)` for critical failures requiring manual intervention.
    async fn compensate(ctx: &mut Ctx, input: &Self::Input) -> Result<CompensationOutcome, Err>;

    /// Get the retry policy for this step.
    ///
    /// Override this to enable retries for transient failures.
    /// Default is no retries - immediate compensation on failure.
    fn retry_policy() -> RetryPolicy {
        RetryPolicy::NoRetry
    }
}

/// Wrapper for storing step inputs in HList.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepWrapper<S: Step<Ctx, Err>, Ctx, Err>
where
    Ctx: Send + Sync,
    Err: Send + Sync,
{
    input: S::Input,
    #[serde(skip)]
    _marker: std::marker::PhantomData<(S, Ctx, Err)>,
}

impl<S, Ctx, Err> StepWrapper<S, Ctx, Err>
where
    S: Step<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync,
{
    /// Create a new step wrapper with the given input.
    pub fn new(input: S::Input) -> Self {
        Self {
            input,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get a reference to the input.
    pub fn input(&self) -> &S::Input {
        &self.input
    }

    /// Execute the step.
    pub async fn execute(&self, ctx: &mut Ctx) -> Result<StepOutcome, Err> {
        S::execute(ctx, &self.input).await
    }

    /// Compensate the step.
    pub async fn compensate(&self, ctx: &mut Ctx) -> Result<CompensationOutcome, Err> {
        S::compensate(ctx, &self.input).await
    }

    /// Get the retry policy.
    pub fn retry_policy(&self) -> RetryPolicy {
        S::retry_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_should_retry() {
        let no_retry = RetryPolicy::NoRetry;
        assert!(!no_retry.should_retry(0));
        assert!(!no_retry.should_retry(1));

        let retry_3 = RetryPolicy::retries(3);
        assert!(retry_3.should_retry(0));
        assert!(retry_3.should_retry(1));
        assert!(retry_3.should_retry(2));
        assert!(!retry_3.should_retry(3));
        assert!(!retry_3.should_retry(4));
    }

    #[test]
    fn retry_policy_backoff() {
        let no_retry = RetryPolicy::NoRetry;
        assert_eq!(no_retry.backoff_ms(), 0);

        let retry_with_backoff = RetryPolicy::retries_with_backoff(3, 100);
        assert_eq!(retry_with_backoff.backoff_ms(), 100);
    }
}
