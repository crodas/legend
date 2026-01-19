#![deny(missing_docs)]

//! Legend — strict composable sagas for sequential workflows.
//!
//! # Design Goals
//!
//! Legend is focused on **compile-time guarantees**:
//!
//! - **HList-based sequences**: Empty programs are compile errors, not runtime panics
//! - **Typestate execution**: Invalid state transitions are caught at compile time
//! - **Strict compensation**: No silent failures - compensation must explicitly succeed or fail
//!
//! # Core Concepts
//!
//! - [`Step`]: A single operation with `execute` and `compensate`
//! - [`Execution`]: Runtime state with typestate tracking (`New`, `Paused`, `Completed`, `Failed`)
//!
// Re-export paste for macros
pub use paste;

// Modules
pub mod execution;
pub mod hlist;
mod macros;
pub mod step;
pub mod store;

// Re-exports for convenience
pub use execution::{
    Completed, Execution, ExecutionPhase, ExecutionResult, ExecutionState, Failed, New, Paused,
    StepStatus, StepTiming,
};
pub use hlist::{BlockResult, HCons, HNil, HSingle, InstructionList};
pub use step::{CompensationOutcome, RetryPolicy, Step, StepOutcome, StepWrapper};
pub use store::{ExecutionId, InMemoryStore, PausedRecord, Store, StoreError};

#[cfg(test)]
mod tests;
