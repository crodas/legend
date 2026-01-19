# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-01-19

### Added

- Initial release
- `Step` trait for defining saga steps with `execute` and `compensate`
- `StepOutcome` enum: `Continue`, `Pause`
- `CompensationOutcome` enum: `Completed`, `Pause`
- `RetryPolicy` for configuring automatic retries
- `Execution` struct with typestate tracking (`New`, `Paused`, `Completed`, `Failed`)
- `ExecutionResult` enum for handling execution outcomes
- HList-based step composition (`HSingle`, `HCons`)
- `legend!` macro for defining saga programs
- `block!` macro for defining reusable step sequences
- `Store` trait for persisting paused executions
- `InMemoryStore` implementation using `parking_lot`
- Step timing tracking with `StepTiming`
- Optional tracing support via `tracing` feature
- Compile-time validation (empty programs don't compile)
- Typestate execution (invalid state transitions are compile errors)

[Unreleased]: https://github.com/cesare/legend/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cesare/legend/releases/tag/v0.1.0
