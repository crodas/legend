//! Heterogeneous list types for compile-time validated instruction sequences.
//!
//! HList (heterogeneous list) is used to enforce non-empty programs at compile time.
//! `HNil` represents an empty list and does NOT implement `InstructionList`,
//! so attempting to build an empty program results in a compile error.

use serde::{Deserialize, Serialize};

use crate::execution::{ExecutionState, StepStatus};
use crate::step::{CompensationOutcome, Step, StepOutcome, StepWrapper};

/// Empty heterogeneous list.
///
/// `HNil` intentionally does NOT implement `InstructionList`.
/// This ensures empty programs cannot be constructed.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct HNil;

/// Non-empty heterogeneous list node.
///
/// `HCons<H, T>` holds a head element and a tail (which can be another HCons or HSingle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HCons<H, T> {
    /// The first element of this list segment.
    pub head: H,
    /// The remaining elements (another HCons or HSingle).
    pub tail: T,
}

impl<H, T> HCons<H, T> {
    /// Create a new HCons with the given head and tail.
    pub fn new(head: H, tail: T) -> Self {
        Self { head, tail }
    }
}

/// Single-element heterogeneous list (base case).
///
/// This is the minimal non-empty list, required because HNil doesn't implement InstructionList.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HSingle<H>(pub H);

impl<H> HSingle<H> {
    /// Create a new single-element list.
    pub fn new(head: H) -> Self {
        Self(head)
    }
}

/// Result of executing a block of instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockResult<E> {
    /// All instructions completed successfully.
    Completed,
    /// Execution was paused (can be resumed).
    Paused,
    /// Execution failed, compensation completed.
    Failed(E),
    /// Execution failed and compensation also failed critically.
    CompensationFailed {
        /// The original error that triggered compensation.
        original_error: E,
        /// The error that occurred during compensation.
        compensation_error: E,
        /// Index of the step where compensation failed.
        failed_at: usize,
    },
}

impl<E> BlockResult<E> {
    /// Returns `true` if all instructions completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    /// Returns `true` if execution was paused.
    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    /// Returns `true` if execution failed (including compensation failures).
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::CompensationFailed { .. })
    }
}

/// Trait for instruction sequences that can be executed.
///
/// This trait is implemented for non-empty HLists (HSingle and HCons).
/// HNil intentionally does NOT implement this trait, ensuring empty
/// programs cannot be constructed.
#[async_trait::async_trait]
pub trait InstructionList<Ctx, Err>: Send + Sync + 'static
where
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Number of steps in this instruction list.
    const LEN: usize;

    /// Execute all instructions in forward order.
    async fn execute_all(&self, ctx: &mut Ctx, state: &mut ExecutionState) -> BlockResult<Err>;

    /// Compensate executed instructions in reverse order.
    async fn compensate_all(
        &self,
        ctx: &mut Ctx,
        state: &mut ExecutionState,
    ) -> Result<CompensationOutcome, Err>;

    /// Execute a single step at the given index.
    async fn execute_at(&self, index: usize, ctx: &mut Ctx) -> Result<StepOutcome, Err>;

    /// Compensate a single step at the given index.
    async fn compensate_at(&self, index: usize, ctx: &mut Ctx) -> Result<CompensationOutcome, Err>;

    /// Get the retry policy for the step at the given index.
    fn retry_policy_at(&self, index: usize) -> crate::step::RetryPolicy;
}

/// Implementation for single-element list (base case).
#[async_trait::async_trait]
impl<S, Ctx, Err> InstructionList<Ctx, Err> for HSingle<StepWrapper<S, Ctx, Err>>
where
    S: Step<Ctx, Err>,
    Ctx: Send + Sync + 'static,
    Err: Send + Sync + Clone + 'static,
{
    const LEN: usize = 1;

    async fn execute_all(&self, ctx: &mut Ctx, state: &mut ExecutionState) -> BlockResult<Err> {
        loop {
            let step_index = state.current_index();
            state.record_step_start(step_index, false);

            #[cfg(feature = "tracing")]
            tracing::info!(step = step_index, "step.start");

            match self.0.execute(ctx).await {
                Ok(StepOutcome::Continue) => {
                    state.record_step_end(StepStatus::Continue);

                    #[cfg(feature = "tracing")]
                    tracing::info!(step = step_index, outcome = "continue", "step.end");

                    state.advance();
                    state.mark_executed();
                    return BlockResult::Completed;
                }
                Ok(StepOutcome::Pause) => {
                    state.record_step_end(StepStatus::Pause);

                    #[cfg(feature = "tracing")]
                    tracing::info!(step = step_index, outcome = "pause", "step.end");

                    state.mark_executed();
                    return BlockResult::Paused;
                }
                Err(e) => {
                    if self.0.retry_policy().should_retry(state.retry_count()) {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            step = step_index,
                            retry = state.retry_count(),
                            "step.retry"
                        );

                        state.increment_retry();
                        continue;
                    }

                    state.record_step_end(StepStatus::Failed);

                    #[cfg(feature = "tracing")]
                    tracing::error!(step = step_index, outcome = "failed", "step.end");

                    return BlockResult::Failed(e);
                }
            }
        }
    }

    async fn compensate_all(
        &self,
        ctx: &mut Ctx,
        state: &mut ExecutionState,
    ) -> Result<CompensationOutcome, Err> {
        state.record_step_start(0, true);

        #[cfg(feature = "tracing")]
        tracing::info!(step = 0, "compensate.start");

        match self.0.compensate(ctx).await {
            Ok(CompensationOutcome::Completed) => {
                state.record_step_end(StepStatus::Compensated);
                #[cfg(feature = "tracing")]
                tracing::info!(step = 0, outcome = "completed", "compensate.end");
                Ok(CompensationOutcome::Completed)
            }
            Ok(CompensationOutcome::Pause) => {
                state.record_step_end(StepStatus::Pause);
                #[cfg(feature = "tracing")]
                tracing::info!(step = 0, outcome = "pause", "compensate.end");
                Ok(CompensationOutcome::Pause)
            }
            Err(e) => {
                state.record_step_end(StepStatus::CompensationFailed);
                #[cfg(feature = "tracing")]
                tracing::error!(step = 0, outcome = "critical", "compensate.end");
                Err(e)
            }
        }
    }

    async fn execute_at(&self, index: usize, ctx: &mut Ctx) -> Result<StepOutcome, Err> {
        if index == 0 {
            self.0.execute(ctx).await
        } else {
            panic!("Index out of bounds: {} >= {}", index, Self::LEN);
        }
    }

    async fn compensate_at(&self, index: usize, ctx: &mut Ctx) -> Result<CompensationOutcome, Err> {
        if index == 0 {
            self.0.compensate(ctx).await
        } else {
            panic!("Index out of bounds: {} >= {}", index, Self::LEN);
        }
    }

    fn retry_policy_at(&self, index: usize) -> crate::step::RetryPolicy {
        if index == 0 {
            self.0.retry_policy()
        } else {
            panic!("Index out of bounds: {} >= {}", index, Self::LEN);
        }
    }
}

/// Implementation for multi-element list (recursive case).
#[async_trait::async_trait]
impl<S, Ctx, Err, T> InstructionList<Ctx, Err> for HCons<StepWrapper<S, Ctx, Err>, T>
where
    S: Step<Ctx, Err>,
    Ctx: Send + Sync + 'static,
    Err: Send + Sync + Clone + 'static,
    T: InstructionList<Ctx, Err>,
{
    const LEN: usize = 1 + T::LEN;

    async fn execute_all(&self, ctx: &mut Ctx, state: &mut ExecutionState) -> BlockResult<Err> {
        let head_index = state.current_index();

        // Execute head first
        loop {
            state.record_step_start(head_index, false);

            #[cfg(feature = "tracing")]
            tracing::info!(step = head_index, "step.start");

            match self.head.execute(ctx).await {
                Ok(StepOutcome::Continue) => {
                    state.record_step_end(StepStatus::Continue);

                    #[cfg(feature = "tracing")]
                    tracing::info!(step = head_index, outcome = "continue", "step.end");

                    state.advance();
                    state.mark_executed();
                    break;
                }
                Ok(StepOutcome::Pause) => {
                    state.record_step_end(StepStatus::Pause);

                    #[cfg(feature = "tracing")]
                    tracing::info!(step = head_index, outcome = "pause", "step.end");

                    state.mark_executed();
                    return BlockResult::Paused;
                }
                Err(e) => {
                    if self.head.retry_policy().should_retry(state.retry_count()) {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            step = head_index,
                            retry = state.retry_count(),
                            "step.retry"
                        );

                        state.increment_retry();
                        continue;
                    }

                    state.record_step_end(StepStatus::Failed);

                    #[cfg(feature = "tracing")]
                    tracing::error!(step = head_index, outcome = "failed", "step.end");

                    return BlockResult::Failed(e);
                }
            }
        }

        // Execute tail
        match self.tail.execute_all(ctx, state).await {
            BlockResult::Completed => BlockResult::Completed,
            BlockResult::Paused => BlockResult::Paused,
            BlockResult::Failed(e) => {
                // Tail failed, compensate head
                state.record_step_start(head_index, true);

                #[cfg(feature = "tracing")]
                tracing::info!(step = head_index, "compensate.start");

                match self.head.compensate(ctx).await {
                    Ok(CompensationOutcome::Completed) => {
                        state.record_step_end(StepStatus::Compensated);

                        #[cfg(feature = "tracing")]
                        tracing::info!(step = head_index, outcome = "completed", "compensate.end");

                        BlockResult::Failed(e)
                    }
                    Ok(CompensationOutcome::Pause) => {
                        state.record_step_end(StepStatus::Pause);

                        #[cfg(feature = "tracing")]
                        tracing::info!(step = head_index, outcome = "pause", "compensate.end");

                        BlockResult::Paused
                    }
                    Err(ce) => {
                        state.record_step_end(StepStatus::CompensationFailed);

                        #[cfg(feature = "tracing")]
                        tracing::error!(step = head_index, outcome = "critical", "compensate.end");

                        BlockResult::CompensationFailed {
                            original_error: e,
                            compensation_error: ce,
                            failed_at: 0,
                        }
                    }
                }
            }
            BlockResult::CompensationFailed {
                original_error,
                compensation_error,
                failed_at,
            } => {
                // Tail compensation failed, still try to compensate head
                state.record_step_start(head_index, true);

                #[cfg(feature = "tracing")]
                tracing::info!(step = head_index, "compensate.start");

                match self.head.compensate(ctx).await {
                    Ok(CompensationOutcome::Completed) | Ok(CompensationOutcome::Pause) => {
                        state.record_step_end(StepStatus::Compensated);

                        #[cfg(feature = "tracing")]
                        tracing::info!(step = head_index, outcome = "completed", "compensate.end");

                        BlockResult::CompensationFailed {
                            original_error,
                            compensation_error,
                            failed_at: failed_at + 1,
                        }
                    }
                    Err(ce) => {
                        state.record_step_end(StepStatus::CompensationFailed);

                        #[cfg(feature = "tracing")]
                        tracing::error!(step = head_index, outcome = "critical", "compensate.end");

                        BlockResult::CompensationFailed {
                            original_error,
                            compensation_error: ce,
                            failed_at: 0,
                        }
                    }
                }
            }
        }
    }

    async fn compensate_all(
        &self,
        ctx: &mut Ctx,
        state: &mut ExecutionState,
    ) -> Result<CompensationOutcome, Err> {
        // Compensate tail first (reverse order)
        match self.tail.compensate_all(ctx, state).await {
            Ok(CompensationOutcome::Completed) => {}
            other => return other,
        }

        // Then compensate head
        state.record_step_start(0, true);

        #[cfg(feature = "tracing")]
        tracing::info!(step = 0, "compensate.start");

        match self.head.compensate(ctx).await {
            Ok(CompensationOutcome::Completed) => {
                state.record_step_end(StepStatus::Compensated);
                #[cfg(feature = "tracing")]
                tracing::info!(step = 0, outcome = "completed", "compensate.end");
                Ok(CompensationOutcome::Completed)
            }
            Ok(CompensationOutcome::Pause) => {
                state.record_step_end(StepStatus::Pause);
                #[cfg(feature = "tracing")]
                tracing::info!(step = 0, outcome = "pause", "compensate.end");
                Ok(CompensationOutcome::Pause)
            }
            Err(e) => {
                state.record_step_end(StepStatus::CompensationFailed);
                #[cfg(feature = "tracing")]
                tracing::error!(step = 0, outcome = "critical", "compensate.end");
                Err(e)
            }
        }
    }

    async fn execute_at(&self, index: usize, ctx: &mut Ctx) -> Result<StepOutcome, Err> {
        if index == 0 {
            self.head.execute(ctx).await
        } else {
            self.tail.execute_at(index - 1, ctx).await
        }
    }

    async fn compensate_at(&self, index: usize, ctx: &mut Ctx) -> Result<CompensationOutcome, Err> {
        if index == 0 {
            self.head.compensate(ctx).await
        } else {
            self.tail.compensate_at(index - 1, ctx).await
        }
    }

    fn retry_policy_at(&self, index: usize) -> crate::step::RetryPolicy {
        if index == 0 {
            self.head.retry_policy()
        } else {
            self.tail.retry_policy_at(index - 1)
        }
    }
}

/// Helper trait to prepend an element to an HList.
pub trait Prepend<H> {
    /// The resulting HList type after prepending.
    type Output;
    /// Prepend an element to the front of this list.
    fn prepend(self, head: H) -> Self::Output;
}

impl<H, T> Prepend<H> for T {
    type Output = HCons<H, T>;
    fn prepend(self, head: H) -> Self::Output {
        HCons::new(head, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hnil_default() {
        let _nil: HNil = HNil;
    }

    #[test]
    fn hsingle_creation() {
        let single = HSingle::new(42);
        assert_eq!(single.0, 42);
    }

    #[test]
    fn hcons_creation() {
        let list = HCons::new(1, HSingle::new(2));
        assert_eq!(list.head, 1);
        assert_eq!(list.tail.0, 2);
    }
}
