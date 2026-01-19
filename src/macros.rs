//! Macros for defining blocks and programs.
//!
//! - `block!`: Define reusable, nestable instruction sequences
//! - `legend!`: Define entry-point programs with builder pattern

/// Define a reusable block of steps.
///
/// A block is a named sequence of steps that can be:
/// - Executed standalone
/// - Nested within other blocks or programs
/// - Used as a step (blocks implement `Step`)
///
/// # Generated Code
///
/// The macro generates:
/// - `{Name}Inputs` struct with fields for each step's input
/// - `{Name}` struct implementing `Block` and `Step` traits
#[macro_export]
macro_rules! block {
    (
        $name:ident <$ctx:ty, $err:ty> {
            $(
                $step_name:ident : $step_type:ty
            ),*
            $(,)?
        }
    ) => {
        $crate::paste::paste! {
            /// Input struct for the block.
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            pub struct [<$name Inputs>] {
                $(
                    pub $step_name: <$step_type as $crate::Step<$ctx, $err>>::Input,
                )*
            }

            /// The block struct.
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            pub struct $name {
                inputs: [<$name Inputs>],
            }

            impl $name {
                /// Create a new block with the given inputs.
                pub fn new(inputs: [<$name Inputs>]) -> Self {
                    Self { inputs }
                }
            }

            // Implement Step for the block so it can be nested
            #[async_trait::async_trait]
            impl $crate::Step<$ctx, $err> for $name {
                type Input = [<$name Inputs>];

                async fn execute(
                    ctx: &mut $ctx,
                    input: &Self::Input,
                ) -> $crate::StepResult<$err> {
                    // Execute each step in order
                    $(
                        match <$step_type as $crate::Step<$ctx, $err>>::execute(ctx, &input.$step_name).await {
                            $crate::StepResult::Continue => {},
                            $crate::StepResult::Pause => return $crate::StepResult::Pause,
                            $crate::StepResult::Fail(e) => return $crate::StepResult::Fail(e),
                        }
                    )*
                    $crate::StepResult::Continue
                }

                async fn compensate(
                    ctx: &mut $ctx,
                    input: &Self::Input,
                ) -> $crate::CompensationResult<$err> {
                    // Compensate in reverse order
                    // Note: This is a simplified version - a full implementation would
                    // track which steps executed and only compensate those
                    $crate::block!(@compensate_reverse ctx, input, $ctx, $err, [$($step_name: $step_type),*])
                }
            }
        }
    };

    // Helper to reverse the compensation order
    (@compensate_reverse $ctx:ident, $input:ident, $ctx_ty:ty, $err_ty:ty, [$first_name:ident: $first_type:ty $(, $rest_name:ident: $rest_type:ty)*]) => {{
        // Compensate rest first (recursively)
        $crate::block!(@compensate_reverse $ctx, $input, $ctx_ty, $err_ty, [$($rest_name: $rest_type),*]);
        // Then compensate first
        match <$first_type as $crate::Step<$ctx_ty, $err_ty>>::compensate($ctx, &$input.$first_name).await {
            $crate::CompensationResult::Completed => {},
            $crate::CompensationResult::Pause => return $crate::CompensationResult::Pause,
            $crate::CompensationResult::Critical(e) => return $crate::CompensationResult::Critical(e),
        }
        $crate::CompensationResult::Completed
    }};

    // Base case - no more steps to compensate
    (@compensate_reverse $ctx:ident, $input:ident, $ctx_ty:ty, $err_ty:ty, []) => {
        // Nothing to compensate
    };
}

/// Define a saga program entry point.
///
/// A program is similar to a block but provides the `build()` method
/// to create an `Execution` instance.
///
/// # Generated Code
///
/// The macro generates:
/// - `{Name}Inputs` struct with fields for each step's input
/// - `{Name}` struct with `new()` and `build()` methods
#[macro_export]
macro_rules! legend {
    (
        $name:ident <$ctx:ty, $err:ty> {
            $(
                $step_name:ident : $step_type:ty
            ),*
            $(,)?
        }
    ) => {
        $crate::paste::paste! {
            /// Input struct for the program.
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            pub struct [<$name Inputs>] {
                $(
                    pub $step_name: <$step_type as $crate::Step<$ctx, $err>>::Input,
                )*
            }

            /// The program struct.
            #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
            pub struct $name {
                inputs: [<$name Inputs>],
            }

            impl $name {
                /// Create a new program with the given inputs.
                pub fn new(inputs: [<$name Inputs>]) -> Self {
                    Self { inputs }
                }

                /// Build an execution from this program.
                pub fn build(self, ctx: $ctx) -> $crate::Execution<
                    $ctx,
                    $err,
                    $crate::legend!(@steps_type $ctx, $err, [$($step_name: $step_type),*]),
                    $crate::New
                >
                where
                    $ctx: Send + Sync + 'static,
                    $err: Send + Sync + Clone + 'static,
                {
                    let steps = $crate::legend!(@build_steps self.inputs, $ctx, $err, [$($step_name: $step_type),*]);
                    $crate::Execution::new(steps, ctx)
                }
            }

            /// Type alias for the steps HList type.
            #[allow(dead_code)]
            pub type [<$name Steps>] = $crate::legend!(@steps_type $ctx, $err, [$($step_name: $step_type),*]);
        }
    };

    // Generate the HList type for steps (reversed for proper execution order)
    (@steps_type $ctx:ty, $err:ty, [$only_name:ident: $only_type:ty]) => {
        $crate::HSingle<$crate::StepWrapper<$only_type, $ctx, $err>>
    };

    (@steps_type $ctx:ty, $err:ty, [$first_name:ident: $first_type:ty, $($rest_name:ident: $rest_type:ty),+]) => {
        $crate::HCons<
            $crate::StepWrapper<$first_type, $ctx, $err>,
            $crate::legend!(@steps_type $ctx, $err, [$($rest_name: $rest_type),+])
        >
    };

    // Build the HList of steps
    (@build_steps $inputs:expr, $ctx:ty, $err:ty, [$only_name:ident: $only_type:ty]) => {
        $crate::HSingle::new($crate::StepWrapper::<$only_type, $ctx, $err>::new($inputs.$only_name.clone()))
    };

    (@build_steps $inputs:expr, $ctx:ty, $err:ty, [$first_name:ident: $first_type:ty, $($rest_name:ident: $rest_type:ty),+]) => {
        $crate::HCons::new(
            $crate::StepWrapper::<$first_type, $ctx, $err>::new($inputs.$first_name.clone()),
            $crate::legend!(@build_steps $inputs, $ctx, $err, [$($rest_name: $rest_type),+])
        )
    };
}
