//! Retry policy tests.
//!
//! Tests for the retry mechanism when steps fail with transient errors.

use crate::ExecutionResult;

use super::common::{MathContext, MathError, MathWithFlaky, MathWithFlakyInputs};

/// Test that retries work with transient failures.
///
/// Verifies:
/// - Flaky step fails 2 times
/// - Step is retried automatically
/// - On 3rd attempt, succeeds
/// - Final status is Completed
#[tokio::test]
async fn retry_succeeds_after_transient_failures() {
    let saga = MathWithFlaky::new(MathWithFlakyInputs {
        add: ("a".into(), 1, 1), // a = 2
        flaky: "x".into(),
        halt: (),
    });

    let mut ctx = MathContext::default();
    ctx.fail_count = 2; // Fail first 2 attempts, succeed on 3rd

    let exec = saga.build(ctx);

    match exec.start().await {
        ExecutionResult::Completed(e) => {
            assert_eq!(e.context().r.get("a").copied(), Some(2));
            assert_eq!(e.context().r.get("x").copied(), Some(1));
        }
        other => panic!(
            "Expected Completed, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

/// Test that exceeding max retries triggers compensation.
///
/// Verifies:
/// - Flaky step fails more than max retries (3)
/// - After exhausting retries, compensation is triggered
/// - Previous steps are rolled back
/// - Final status is Failed
#[tokio::test]
async fn exceeding_retries_triggers_compensation() {
    let saga = MathWithFlaky::new(MathWithFlakyInputs {
        add: ("a".into(), 1, 1), // a = 2
        flaky: "x".into(),
        halt: (),
    });

    let mut ctx = MathContext::default();
    ctx.fail_count = 5; // More than max retries (3)

    let exec = saga.build(ctx);

    match exec.start().await {
        ExecutionResult::Failed(e, err) => {
            assert_eq!(err, MathError::Transient);
            // Previous step should be compensated
            assert!(!e.context().r.contains_key("a"));
        }
        other => panic!("Expected Failed, got {:?}", std::mem::discriminant(&other)),
    }
}

/// Test retry policy configuration.
#[test]
fn retry_policy_configuration() {
    use crate::RetryPolicy;

    // No retry policy
    let no_retry = RetryPolicy::NoRetry;
    assert!(!no_retry.should_retry(0));
    assert!(!no_retry.should_retry(1));
    assert_eq!(no_retry.backoff_ms(), 0);

    // Retry with limit
    let retry_3 = RetryPolicy::retries(3);
    assert!(retry_3.should_retry(0));
    assert!(retry_3.should_retry(1));
    assert!(retry_3.should_retry(2));
    assert!(!retry_3.should_retry(3));
    assert!(!retry_3.should_retry(4));

    // Retry with backoff
    let retry_with_backoff = RetryPolicy::retries_with_backoff(3, 100);
    assert!(retry_with_backoff.should_retry(0));
    assert_eq!(retry_with_backoff.backoff_ms(), 100);
}
