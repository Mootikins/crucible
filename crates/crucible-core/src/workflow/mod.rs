//! Workflow execution engine (Phase 3a, pure logic).
//!
//! This module is the *orchestrator* portion of Phase 3a: it walks a
//! [`WorkflowDoc`][crate::parser::types::WorkflowDoc] produced by the
//! parser, dispatches each step through a [`DispatchTable`] keyed by the
//! `[type:: ...]` heading attribute, threads a per-session
//! [`OutputScope`] through the run, and emits [`WorkflowEvent`]s.
//!
//! Per the plan (`thoughts/shared/plans/workflows_2026-04-22-2030.md`)
//! this is deliberately structured to make Phase 3b (port orchestration
//! to Lua) a mechanical translation — dispatch table not match arms, no
//! ambient state, hook events always emitted, and every primitive that a
//! handler calls goes through a narrow trait surface.
//!
//! # Slice scope
//!
//! This first slice ships two stdlib handlers: `default` (inline — the
//! workflow's agent runs one turn with the step body as prompt) and
//! `gate` (pause, wait for explicit approval). `fan` and `ralph` are
//! deferred to the next slice. The `default` handler currently
//! produces a placeholder output rather than invoking a real LLM; the
//! daemon will swap in a real implementation without changing the
//! engine surface.

mod engine;
mod events;
mod handler;
mod stdlib;

pub use engine::{PendingGate, WorkflowExecution, WorkflowStatus};
pub use events::WorkflowEvent;
pub use handler::{DispatchTable, ExecContext, StepHandler, StepOutcome};
pub use stdlib::{stdlib_dispatch, DefaultHandler, GateHandler};

use std::collections::HashMap;

/// Per-execution output scope: named outputs produced by steps with a
/// `-> name` suffix. Values are stored as JSON so Lua and other
/// consumers can serialize through the same shape.
pub type OutputScope = HashMap<String, serde_json::Value>;
