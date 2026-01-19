//! Comprehensive saga demo showing happy and unhappy paths.
//!
//! Run with: cargo run --example demo

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use legend::step::{CompensationOutcome, RetryPolicy, Step, StepOutcome};
use legend::{legend, ExecutionResult};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Context and Error types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OrderContext {
    order_id: Option<String>,
    payment_id: Option<String>,
    inventory_reserved: bool,
    notification_sent: bool,
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
enum OrderError {
    #[error("Payment failed: {0}")]
    PaymentFailed(String),
    #[error("Inventory unavailable")]
    InventoryUnavailable,
    #[error("Notification service down")]
    NotificationFailed,
}

// ============================================================================
// Step implementations
// ============================================================================

/// Step 1: Create order record
pub struct CreateOrder;

#[async_trait]
impl Step<OrderContext, OrderError> for CreateOrder {
    type Input = String; // customer_id

    async fn execute(ctx: &mut OrderContext, input: &String) -> Result<StepOutcome, OrderError> {
        println!("  [CreateOrder] Creating order for customer: {}", input);
        tokio::time::sleep(Duration::from_millis(100)).await;

        ctx.order_id = Some(format!("ORD-{}-001", input));
        println!("  [CreateOrder] Order created: {:?}", ctx.order_id);
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut OrderContext,
        _input: &String,
    ) -> Result<CompensationOutcome, OrderError> {
        println!(
            "  [CreateOrder] COMPENSATING - Canceling order: {:?}",
            ctx.order_id
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        ctx.order_id = None;
        Ok(CompensationOutcome::Completed)
    }
}

/// Step 2: Reserve inventory
pub struct ReserveInventory;

#[async_trait]
impl Step<OrderContext, OrderError> for ReserveInventory {
    type Input = u32; // quantity

    async fn execute(ctx: &mut OrderContext, input: &u32) -> Result<StepOutcome, OrderError> {
        println!("  [ReserveInventory] Reserving {} items...", input);
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Simulate inventory check
        if *input > 100 {
            println!("  [ReserveInventory] FAILED - Not enough inventory!");
            return Err(OrderError::InventoryUnavailable);
        }

        ctx.inventory_reserved = true;
        println!("  [ReserveInventory] Reserved {} items", input);
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut OrderContext,
        input: &u32,
    ) -> Result<CompensationOutcome, OrderError> {
        println!(
            "  [ReserveInventory] COMPENSATING - Releasing {} items",
            input
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        ctx.inventory_reserved = false;
        Ok(CompensationOutcome::Completed)
    }
}

/// Step 3: Process payment (with retry)
pub struct ProcessPayment;

// Track retry attempts for demo
static PAYMENT_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

#[async_trait]
impl Step<OrderContext, OrderError> for ProcessPayment {
    type Input = PaymentInfo;

    async fn execute(
        ctx: &mut OrderContext,
        input: &PaymentInfo,
    ) -> Result<StepOutcome, OrderError> {
        let attempt = PAYMENT_ATTEMPTS.fetch_add(1, Ordering::SeqCst) + 1;
        println!(
            "  [ProcessPayment] Attempt {} - Processing ${:.2}...",
            attempt, input.amount
        );
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Simulate transient failures (first 2 attempts fail, 3rd succeeds)
        if input.simulate_transient_failure && attempt < 3 {
            println!("  [ProcessPayment] TRANSIENT FAILURE - Will retry...");
            return Err(OrderError::PaymentFailed("Gateway timeout".into()));
        }

        // Simulate permanent failure
        if input.simulate_permanent_failure {
            println!("  [ProcessPayment] PERMANENT FAILURE - Card declined!");
            return Err(OrderError::PaymentFailed("Card declined".into()));
        }

        ctx.payment_id = Some(format!("PAY-{}", uuid::Uuid::new_v4()));
        println!(
            "  [ProcessPayment] Payment successful: {:?}",
            ctx.payment_id
        );
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut OrderContext,
        input: &PaymentInfo,
    ) -> Result<CompensationOutcome, OrderError> {
        println!(
            "  [ProcessPayment] COMPENSATING - Refunding ${:.2}",
            input.amount
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        ctx.payment_id = None;
        Ok(CompensationOutcome::Completed)
    }

    fn retry_policy() -> RetryPolicy {
        RetryPolicy::Retry {
            max_attempts: 3,
            backoff_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInfo {
    amount: f64,
    simulate_transient_failure: bool,
    simulate_permanent_failure: bool,
}

/// Step 4: Send notification
pub struct SendNotification;

#[async_trait]
impl Step<OrderContext, OrderError> for SendNotification {
    type Input = String; // email

    async fn execute(ctx: &mut OrderContext, input: &String) -> Result<StepOutcome, OrderError> {
        println!("  [SendNotification] Sending confirmation to {}...", input);
        tokio::time::sleep(Duration::from_millis(100)).await;

        ctx.notification_sent = true;
        println!("  [SendNotification] Email sent!");
        Ok(StepOutcome::Continue)
    }

    async fn compensate(
        ctx: &mut OrderContext,
        input: &String,
    ) -> Result<CompensationOutcome, OrderError> {
        println!(
            "  [SendNotification] COMPENSATING - Sending cancellation to {}",
            input
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        ctx.notification_sent = false;
        Ok(CompensationOutcome::Completed)
    }
}

// ============================================================================
// Program definition using legend! macro
// ============================================================================

legend! {
    OrderWorkflow<OrderContext, OrderError> {
        create_order: CreateOrder,
        reserve_inventory: ReserveInventory,
        process_payment: ProcessPayment,
        send_notification: SendNotification,
    }
}

// ============================================================================
// Demo scenarios
// ============================================================================

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           Legend Saga Demo                                   ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Scenario 1: Happy path - everything works
    run_happy_path().await;

    // Scenario 2: Unhappy path - inventory failure triggers compensation
    run_inventory_failure().await;

    // Scenario 3: Payment with transient failures (retries succeed)
    run_payment_retry_success().await;

    // Scenario 4: Payment permanent failure triggers compensation
    run_payment_permanent_failure().await;

    println!("\n✓ All demos completed!");
}

async fn run_happy_path() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Scenario 1: Happy Path - All steps succeed                   │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let saga = OrderWorkflow::new(OrderWorkflowInputs {
        create_order: "CUST-123".to_string(),
        reserve_inventory: 5,
        process_payment: PaymentInfo {
            amount: 99.99,
            simulate_transient_failure: false,
            simulate_permanent_failure: false,
        },
        send_notification: "customer@example.com".to_string(),
    });

    let execution = saga.build(OrderContext::default());

    match execution.start().await {
        ExecutionResult::Completed(e) => {
            println!("\n  ✓ Order completed successfully!");
            println!("    Final context: {:?}\n", e.context());
        }
        ExecutionResult::Failed(_, err) => {
            println!("\n  ✗ Order failed: {}\n", err);
        }
        _ => {}
    }
}

async fn run_inventory_failure() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Scenario 2: Inventory Failure - Triggers compensation        │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let saga = OrderWorkflow::new(OrderWorkflowInputs {
        create_order: "CUST-456".to_string(),
        reserve_inventory: 999, // Too many! Will fail
        process_payment: PaymentInfo {
            amount: 999.99,
            simulate_transient_failure: false,
            simulate_permanent_failure: false,
        },
        send_notification: "customer@example.com".to_string(),
    });

    let execution = saga.build(OrderContext::default());

    match execution.start().await {
        ExecutionResult::Completed(e) => {
            println!("\n  ✓ Order completed: {:?}\n", e.context());
        }
        ExecutionResult::Failed(e, err) => {
            println!("\n  ✗ Order failed (after compensation): {}", err);
            println!("    Context after rollback: {:?}\n", e.context());
        }
        _ => {}
    }
}

async fn run_payment_retry_success() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Scenario 3: Payment Retry - Transient failures, then success │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    // Reset counter
    PAYMENT_ATTEMPTS.store(0, Ordering::SeqCst);

    let saga = OrderWorkflow::new(OrderWorkflowInputs {
        create_order: "CUST-789".to_string(),
        reserve_inventory: 3,
        process_payment: PaymentInfo {
            amount: 49.99,
            simulate_transient_failure: true, // Will fail twice, succeed on 3rd
            simulate_permanent_failure: false,
        },
        send_notification: "customer@example.com".to_string(),
    });

    let execution = saga.build(OrderContext::default());

    match execution.start().await {
        ExecutionResult::Completed(e) => {
            println!("\n  ✓ Order completed after retries!");
            println!("    Final context: {:?}\n", e.context());
        }
        ExecutionResult::Failed(_, err) => {
            println!("\n  ✗ Order failed: {}\n", err);
        }
        _ => {}
    }
}

async fn run_payment_permanent_failure() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Scenario 4: Payment Permanent Failure - Full compensation    │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    // Reset counter
    PAYMENT_ATTEMPTS.store(0, Ordering::SeqCst);

    let saga = OrderWorkflow::new(OrderWorkflowInputs {
        create_order: "CUST-000".to_string(),
        reserve_inventory: 10,
        process_payment: PaymentInfo {
            amount: 199.99,
            simulate_transient_failure: false,
            simulate_permanent_failure: true, // Will fail permanently
        },
        send_notification: "customer@example.com".to_string(),
    });

    let execution = saga.build(OrderContext::default());

    match execution.start().await {
        ExecutionResult::Completed(e) => {
            println!("\n  ✓ Order completed: {:?}\n", e.context());
        }
        ExecutionResult::Failed(e, err) => {
            println!("\n  ✗ Order failed (after compensation): {}", err);
            println!("    Context after rollback: {:?}\n", e.context());
        }
        _ => {}
    }
}
