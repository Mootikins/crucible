//! Bundled stdlib step handlers: `default` (inline) and `gate`.
//!
//! This slice keeps the `default` handler pure — it synthesizes a
//! placeholder output instead of driving an LLM. The daemon will swap
//! in a real implementation (`DaemonInlineHandler` or similar) without
//! touching the dispatch table shape.

use crate::workflow::handler::{DispatchTable, ExecContext, StepHandler, StepOutcome};

/// Inline (same-session) step handler for steps without a `[type:: ...]`
/// attribute. Produces a placeholder output; real LLM invocation comes
/// in the next slice via a daemon-provided handler that overrides this.
pub struct DefaultHandler;

impl StepHandler for DefaultHandler {
    fn execute(&self, ctx: &ExecContext<'_>) -> StepOutcome {
        let output = ctx.step.output.as_ref().map(|name| {
            serde_json::json!({
                "placeholder": true,
                "produced_by": ctx.step_id,
                "output_name": name,
                "prompt_preview": truncate(&ctx.step.body, 120),
            })
        });
        StepOutcome::Advance { output }
    }
}

/// Halts execution; caller must `approve_gate` to continue.
///
/// The gate ID the engine yields is `{step_id}.gate0` — one gate per
/// step in this slice. Multi-gate-per-step support folds in when we
/// have a real handler for `fan`/`ralph`.
pub struct GateHandler;

impl StepHandler for GateHandler {
    fn execute(&self, ctx: &ExecContext<'_>) -> StepOutcome {
        // Prefer an explicit gate title from a callout inside the body.
        let gate_title = ctx
            .step
            .gates
            .first()
            .and_then(|g| g.title.clone())
            .or_else(|| {
                ctx.step
                    .gates
                    .first()
                    .map(|g| g.content.lines().next().unwrap_or("").to_string())
            })
            .filter(|s| !s.is_empty())
            .or_else(|| Some(ctx.step.title.clone()).filter(|s| !s.is_empty()));

        StepOutcome::YieldForApproval {
            gate_id: format!("{}.gate0", ctx.step_id),
            gate_title,
        }
    }
}

/// Stdlib dispatch table: `gate` plus the default handler for untyped
/// steps. Extend by calling [`DispatchTable::register`] before passing
/// it to `WorkflowExecution::new`.
pub fn stdlib_dispatch() -> DispatchTable {
    let mut table = DispatchTable::new(Box::new(DefaultHandler));
    table.register("gate", Box::new(GateHandler));
    table
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
