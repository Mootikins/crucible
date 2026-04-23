//! Step handler trait and dispatch table.
//!
//! Per the plan's Phase 3a rules: dispatch is table-driven, not
//! pattern-matched, and handlers see only an explicit [`ExecContext`] —
//! no thread-locals or session globals. This keeps the surface
//! translatable into Lua bindings later.

use crate::parser::types::{ValidationEntry, WorkflowStep};
use crate::workflow::OutputScope;
use std::collections::HashMap;

/// Why the engine called a handler — handler decides what to return.
pub enum StepOutcome {
    /// Step succeeded. Optional output is bound under the step's
    /// `-> name` suffix if that's set on the heading.
    Advance { output: Option<serde_json::Value> },
    /// Step is a gate (or a composite that internally hit a gate).
    /// Engine records the gate as pending and stops executing until
    /// `approve_gate` is called.
    YieldForApproval {
        gate_id: String,
        gate_title: Option<String>,
    },
    /// Step failed; engine marks the workflow as Failed.
    Fail { reason: String },
}

/// Context handed to a step handler on each invocation.
///
/// Carries *only* what the handler might need — step itself, the
/// already-committed output scope (read-only), and the workflow's
/// validation list (for agent context priming). Handlers never reach
/// into session state directly; that's the daemon's job.
pub struct ExecContext<'a> {
    pub step: &'a WorkflowStep,
    /// Depth-first path from the workflow root, e.g. `"0.1.2"` for
    /// `doc.steps[0].children[1].children[2]`. Stable across runs.
    pub step_id: &'a str,
    pub scope: &'a OutputScope,
    pub validations: &'a [ValidationEntry],
}

/// Step-type handler. Keyed by the `[type:: X]` attribute on a heading
/// (or the sentinel `""` key for the default/no-type handler).
pub trait StepHandler: Send + Sync {
    fn execute(&self, ctx: &ExecContext<'_>) -> StepOutcome;
}

/// Handler lookup by step-type string. Missing types fall back to the
/// `default` handler so authors can add `[type:: custom-foo]` without
/// blocking forward progress while the Lua executor is being developed.
pub struct DispatchTable {
    pub handlers: HashMap<String, Box<dyn StepHandler>>,
    pub default: Box<dyn StepHandler>,
}

impl DispatchTable {
    pub fn new(default: Box<dyn StepHandler>) -> Self {
        Self {
            handlers: HashMap::new(),
            default,
        }
    }

    pub fn register(&mut self, type_name: impl Into<String>, handler: Box<dyn StepHandler>) {
        self.handlers.insert(type_name.into(), handler);
    }

    pub fn resolve(&self, step_type: Option<&str>) -> &dyn StepHandler {
        step_type
            .and_then(|t| self.handlers.get(t))
            .map(|h| &**h)
            .unwrap_or(&*self.default)
    }
}
