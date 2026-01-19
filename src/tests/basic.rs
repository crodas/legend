//! Basic execution tests.
//!
//! Tests for successful program execution without errors.

use crate::ExecutionResult;

use super::common::{
    Math, MathContext, MathInputs, SingleAdd, SingleAddInputs, SingleHalt, SingleHaltInputs,
};

/// Test that a simple single-step program completes successfully.
#[tokio::test]
async fn single_step_completes() {
    let saga = SingleHalt::new(SingleHaltInputs { halt: () });
    let exec = saga.build(MathContext::default());

    match exec.start().await {
        ExecutionResult::Completed(e) => {
            assert!(e.context().r.is_empty());
        }
        other => panic!(
            "Expected Completed, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

/// Test that the Add step correctly computes and stores results.
#[tokio::test]
async fn add_step_works() {
    let saga = SingleAdd::new(SingleAddInputs {
        add: ("x".into(), 5, 3),
    });
    let exec = saga.build(MathContext::default());

    match exec.start().await {
        ExecutionResult::Completed(e) => {
            assert_eq!(e.context().r.get("x").copied(), Some(8));
        }
        other => panic!(
            "Expected Completed, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

/// Test that a sequence of multiple operations completes successfully.
///
/// Verifies:
/// - Steps execute in order
/// - Each step's result is stored correctly
/// - Existing context state is preserved
#[tokio::test]
async fn sequence_completes() {
    let saga = Math::new(MathInputs {
        add: ("foo".into(), 5, 5),  // foo = 10
        sub: ("bar".into(), 10, 3), // bar = 7
        halt: (),
    });

    let mut ctx = MathContext::default();
    ctx.r.insert("existing".into(), 99);

    let exec = saga.build(ctx);

    match exec.start().await {
        ExecutionResult::Completed(e) => {
            let ctx = e.context();
            assert_eq!(ctx.r.get("foo").copied(), Some(10));
            assert_eq!(ctx.r.get("bar").copied(), Some(7));
            assert_eq!(ctx.r.get("existing").copied(), Some(99));
        }
        other => panic!(
            "Expected Completed, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}
