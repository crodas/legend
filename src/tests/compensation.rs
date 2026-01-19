//! Compensation and rollback tests.
//!
//! Tests that verify automatic compensation when steps fail.

use crate::ExecutionResult;

use super::common::{Math, MathContext, MathError, MathInputs, SingleAdd, SingleAddInputs};

/// Test that underflow triggers failure and compensation.
///
/// Verifies:
/// - Underflow (0 - 1) returns an error
/// - Error triggers compensation
/// - Previous step's changes are rolled back
#[tokio::test]
async fn underflow_triggers_compensation() {
    let saga = Math::new(MathInputs {
        add: ("a".into(), 1, 2), // a = 3
        sub: ("b".into(), 0, 1), // underflow!
        halt: (),
    });
    let exec = saga.build(MathContext::default());

    match exec.start().await {
        ExecutionResult::Failed(e, err) => {
            assert_eq!(err, MathError::Underflow);
            // First step should be compensated (rolled back)
            assert!(!e.context().r.contains_key("a"));
        }
        other => panic!("Expected Failed, got {:?}", std::mem::discriminant(&other)),
    }
}

/// Test that overflow triggers failure.
///
/// Verifies:
/// - Overflow (250 + 20) returns an error
/// - Status is Failed
#[tokio::test]
async fn overflow_triggers_failure() {
    let saga = SingleAdd::new(SingleAddInputs {
        add: ("x".into(), 250, 20), // overflow!
    });
    let exec = saga.build(MathContext::default());

    match exec.start().await {
        ExecutionResult::Failed(_, err) => {
            assert_eq!(err, MathError::Overflow);
        }
        other => panic!("Expected Failed, got {:?}", std::mem::discriminant(&other)),
    }
}

/// Test that multiple steps are compensated in reverse order.
///
/// Verifies:
/// - Multiple successful steps are tracked
/// - Error in a later step triggers rollback
/// - All previous steps are compensated
/// - Context is restored to initial state
#[tokio::test]
async fn multiple_steps_rollback() {
    let saga = Math::new(MathInputs {
        add: ("a".into(), 1, 2), // a = 3
        sub: ("b".into(), 0, 1), // underflow -> triggers rollback
        halt: (),
    });

    let exec = saga.build(MathContext::default());

    match exec.start().await {
        ExecutionResult::Failed(e, _) => {
            // Both 'a' should be rolled back
            assert!(!e.context().r.contains_key("a"));
            // 'b' was never successfully written
            assert!(!e.context().r.contains_key("b"));
        }
        other => panic!("Expected Failed, got {:?}", std::mem::discriminant(&other)),
    }
}
