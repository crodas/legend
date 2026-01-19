//! Pause and resume tests.
//!
//! Tests for the pause/resume workflow with serialization support.

use crate::ExecutionResult;

use super::common::{MathContext, SingleAdd, SingleAddInputs};

/// Test that execution can be paused and the state is correct.
///
/// Verifies:
/// - Step can return Pause to suspend execution
/// - Paused execution preserves computed state
/// - Paused execution can be serialized
#[tokio::test]
async fn pause_preserves_state() {
    let saga = SingleAdd::new(SingleAddInputs {
        add: ("x".into(), 1, 2),
    });

    let mut ctx = MathContext::default();
    ctx.pause_after = Some(0); // pause immediately after execute

    let exec = saga.build(ctx);

    match exec.start().await {
        ExecutionResult::Paused(e) => {
            // Value should be computed
            assert_eq!(e.context().r.get("x").copied(), Some(3));
        }
        other => panic!("Expected Paused, got {:?}", std::mem::discriminant(&other)),
    }
}

/// Test that paused execution can be serialized.
///
/// Verifies:
/// - Paused execution serializes to JSON
/// - Serialized state contains the computed values
#[tokio::test]
async fn paused_execution_serializes() {
    let saga = SingleAdd::new(SingleAddInputs {
        add: ("x".into(), 1, 2),
    });

    let mut ctx = MathContext::default();
    ctx.pause_after = Some(0);

    let exec = saga.build(ctx);

    match exec.start().await {
        ExecutionResult::Paused(e) => {
            let json = serde_json::to_string(&e).expect("should serialize");

            // Verify key data is in the JSON
            assert!(json.contains("\"x\""), "JSON should contain register name");
            assert!(json.contains("3"), "JSON should contain computed value");
        }
        other => panic!("Expected Paused, got {:?}", std::mem::discriminant(&other)),
    }
}
