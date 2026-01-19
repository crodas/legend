//! Execution state types with typestate pattern.
//!
//! This module provides compile-time enforced state transitions using Rust's type system.
//! Invalid transitions (like resuming a completed execution) are caught at compile time.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use crate::hlist::{BlockResult, InstructionList};
use crate::store::now_millis;

// ============================================================================
// Typestate Markers
// ============================================================================

/// Marker: Execution has not started yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct New;

/// Marker: Execution is paused and can be resumed.
#[derive(Debug, Clone, Copy)]
pub struct Paused;

/// Marker: Execution completed successfully.
#[derive(Debug, Clone, Copy)]
pub struct Completed;

/// Marker: Execution failed (after compensation).
#[derive(Debug, Clone, Copy)]
pub struct Failed;

// ============================================================================
// Execution Phase (Runtime)
// ============================================================================

/// The current phase of execution (runtime tracking).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExecutionPhase {
    /// Executing instructions in forward order.
    #[default]
    Forward,
    /// Compensating instructions in reverse order.
    Compensating,
}

// ============================================================================
// Step Timing
// ============================================================================

/// Outcome of a step execution or compensation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    /// Step continued to the next step.
    Continue,
    /// Step paused execution.
    Pause,
    /// Step failed with an error.
    Failed,
    /// Step was compensated successfully.
    Compensated,
    /// Step compensation failed critically.
    CompensationFailed,
}

/// Timing information for a single step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTiming {
    /// Index of the step.
    pub step_index: usize,
    /// Whether this is an execution or compensation.
    pub is_compensation: bool,
    /// When the step started (Unix timestamp ms).
    pub started_at: u64,
    /// When the step completed (Unix timestamp ms), if completed.
    pub completed_at: Option<u64>,
    /// Outcome of the step, if completed.
    pub outcome: Option<StepStatus>,
}

impl StepTiming {
    /// Create a new step timing record.
    pub fn new(step_index: usize, is_compensation: bool) -> Self {
        Self {
            step_index,
            is_compensation,
            started_at: now_millis(),
            completed_at: None,
            outcome: None,
        }
    }

    /// Mark the step as completed with the given outcome.
    pub fn complete(&mut self, outcome: StepStatus) {
        self.completed_at = Some(now_millis());
        self.outcome = Some(outcome);
    }

    /// Get the duration in milliseconds, if completed.
    pub fn duration_ms(&self) -> Option<u64> {
        self.completed_at
            .map(|end| end.saturating_sub(self.started_at))
    }
}

// ============================================================================
// Execution State (Internal)
// ============================================================================

/// Internal execution state - not directly accessible to users.
///
/// Tracks position, retry count, phase, and timing during execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionState {
    /// Current instruction index.
    index: usize,
    /// Number of instructions that have been executed (for compensation bounds).
    executed: usize,
    /// Current retry attempt for the current instruction.
    retry_count: u8,
    /// Current execution phase.
    phase: ExecutionPhase,
    /// When the execution started (Unix timestamp ms).
    started_at: Option<u64>,
    /// Timing records for each step.
    step_timings: Vec<StepTiming>,
}

impl ExecutionState {
    /// Create a new execution state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current instruction index.
    pub fn current_index(&self) -> usize {
        self.index
    }

    /// Get the number of executed instructions.
    pub fn executed_count(&self) -> usize {
        self.executed
    }

    /// Get the current retry count.
    pub fn retry_count(&self) -> u8 {
        self.retry_count
    }

    /// Get the current phase.
    pub fn phase(&self) -> ExecutionPhase {
        self.phase
    }

    /// Advance to the next instruction.
    pub fn advance(&mut self) {
        self.index += 1;
        self.retry_count = 0;
    }

    /// Mark that an instruction was executed (for compensation tracking).
    pub fn mark_executed(&mut self) {
        self.executed = self.executed.max(self.index);
    }

    /// Increment the retry counter.
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Begin compensation phase.
    pub fn begin_compensation(&mut self) {
        self.phase = ExecutionPhase::Compensating;
    }

    /// Check if in compensation phase.
    pub fn is_compensating(&self) -> bool {
        self.phase == ExecutionPhase::Compensating
    }

    /// Mark the execution as started.
    pub fn mark_started(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(now_millis());
        }
    }

    /// Get when the execution started.
    pub fn started_at(&self) -> Option<u64> {
        self.started_at
    }

    /// Record the start of a step execution.
    pub fn record_step_start(&mut self, step_index: usize, is_compensation: bool) {
        self.step_timings
            .push(StepTiming::new(step_index, is_compensation));
    }

    /// Record the end of a step execution.
    pub fn record_step_end(&mut self, outcome: StepStatus) {
        if let Some(timing) = self.step_timings.last_mut() {
            timing.complete(outcome);
        }
    }

    /// Get all step timing records.
    pub fn step_timings(&self) -> &[StepTiming] {
        &self.step_timings
    }

    /// Get the timing for a specific step by index.
    pub fn timing_for_step(&self, step_index: usize, is_compensation: bool) -> Option<&StepTiming> {
        self.step_timings
            .iter()
            .find(|t| t.step_index == step_index && t.is_compensation == is_compensation)
    }
}

// ============================================================================
// Execution Result
// ============================================================================

/// Result of running or resuming an execution.
///
/// This enum represents the possible outcomes after calling `start()` or `resume()`.
pub enum ExecutionResult<Ctx, Err, Steps>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Execution paused and can be resumed.
    Paused(Execution<Ctx, Err, Steps, Paused>),
    /// Execution completed successfully.
    Completed(Execution<Ctx, Err, Steps, Completed>),
    /// Execution failed (after compensation).
    Failed(Execution<Ctx, Err, Steps, Failed>, Err),
    /// Execution failed and compensation also failed.
    CompensationFailed {
        /// The execution in failed state.
        execution: Execution<Ctx, Err, Steps, Failed>,
        /// The original error that triggered compensation.
        original_error: Err,
        /// The error that occurred during compensation.
        compensation_error: Err,
        /// Index of the step where compensation failed.
        failed_at: usize,
    },
}

impl<Ctx, Err, Steps> ExecutionResult<Ctx, Err, Steps>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Returns `true` if the execution is paused.
    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused(_))
    }

    /// Returns `true` if the execution completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }

    /// Returns `true` if the execution failed (including compensation failures).
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_, _) | Self::CompensationFailed { .. })
    }
}

// ============================================================================
// Execution (Typestate)
// ============================================================================

/// A saga execution with compile-time state tracking.
///
/// The `State` type parameter enforces valid operations:
/// - `Execution<..., New>`: Can call `start()`
/// - `Execution<..., Paused>`: Can call `resume()` or serialize
/// - `Execution<..., Completed>`: Can access final context
/// - `Execution<..., Failed>`: Can access error and context
#[derive(Serialize, Deserialize)]
#[serde(
    bound = "Steps: Serialize + serde::de::DeserializeOwned, Ctx: Serialize + serde::de::DeserializeOwned, Err: Serialize + serde::de::DeserializeOwned"
)]
pub struct Execution<Ctx, Err, Steps, State = New>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    steps: Steps,
    ctx: Ctx,
    state: ExecutionState,
    #[serde(skip)]
    _marker: PhantomData<(Err, State)>,
}

impl<Ctx, Err, Steps> Execution<Ctx, Err, Steps, New>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Create a new execution in the `New` state.
    pub fn new(steps: Steps, ctx: Ctx) -> Self {
        Self {
            steps,
            ctx,
            state: ExecutionState::new(),
            _marker: PhantomData,
        }
    }

    /// Start execution.
    ///
    /// Runs the saga from the beginning until completion, pause, or failure.
    pub async fn start(self) -> ExecutionResult<Ctx, Err, Steps> {
        self.run_internal().await
    }
}

impl<Ctx, Err, Steps> Execution<Ctx, Err, Steps, Paused>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Resume a paused execution.
    ///
    /// Continues from where the execution was paused.
    pub async fn resume(self) -> ExecutionResult<Ctx, Err, Steps> {
        self.run_internal().await
    }

    /// Get a reference to the context.
    pub fn context(&self) -> &Ctx {
        &self.ctx
    }
}

impl<Ctx, Err, Steps> Execution<Ctx, Err, Steps, Completed>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Get a reference to the final context.
    pub fn context(&self) -> &Ctx {
        &self.ctx
    }

    /// Consume the execution and return the final context.
    pub fn into_context(self) -> Ctx {
        self.ctx
    }
}

impl<Ctx, Err, Steps> Execution<Ctx, Err, Steps, Failed>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    /// Get a reference to the context (for inspection).
    pub fn context(&self) -> &Ctx {
        &self.ctx
    }

    /// Consume the execution and return the context.
    pub fn into_context(self) -> Ctx {
        self.ctx
    }
}

// Internal implementation for both start and resume
impl<Ctx, Err, Steps, State> Execution<Ctx, Err, Steps, State>
where
    Steps: InstructionList<Ctx, Err>,
    Ctx: Send + Sync,
    Err: Send + Sync + Clone,
{
    async fn run_internal(mut self) -> ExecutionResult<Ctx, Err, Steps> {
        self.state.mark_started();
        match self.steps.execute_all(&mut self.ctx, &mut self.state).await {
            BlockResult::Completed => ExecutionResult::Completed(Execution {
                steps: self.steps,
                ctx: self.ctx,
                state: self.state,
                _marker: PhantomData,
            }),
            BlockResult::Paused => ExecutionResult::Paused(Execution {
                steps: self.steps,
                ctx: self.ctx,
                state: self.state,
                _marker: PhantomData,
            }),
            BlockResult::Failed(e) => ExecutionResult::Failed(
                Execution {
                    steps: self.steps,
                    ctx: self.ctx,
                    state: self.state,
                    _marker: PhantomData,
                },
                e,
            ),
            BlockResult::CompensationFailed {
                original_error,
                compensation_error,
                failed_at,
            } => ExecutionResult::CompensationFailed {
                execution: Execution {
                    steps: self.steps,
                    ctx: self.ctx,
                    state: self.state,
                    _marker: PhantomData,
                },
                original_error,
                compensation_error,
                failed_at,
            },
        }
    }
}
