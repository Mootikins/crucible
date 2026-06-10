//! Workflow execution state machine.
//!
//! `WorkflowExecution` owns a parsed [`WorkflowDoc`], a [`DispatchTable`]
//! for handlers, an output scope, and a cursor into a pre-flattened list
//! of steps. `tick()` advances one slot (preamble gate, step, or
//! child-level step) and returns the resulting status. The daemon
//! drives the loop.
//!
//! Traversal shape (DFS-ordered slots):
//!
//! 1. Each `preamble_gate` yields a `Slot::PreambleGate`.
//! 2. Each step in the tree produces:
//!    - one `Slot::Step` per step (depth-first, parent before children)
//!    - its own gates (before the step body runs) as `Slot::StepGate`
//!
//! Gates are dispatched through the handler registered for `gate`
//! (default: [`GateHandler`][super::GateHandler]). That keeps the
//! handler table the single source of step-type behaviour — a user can
//! override `gate` from Lua and the engine is oblivious.

use crate::parser::types::{Gate, WorkflowDoc, WorkflowStep};
use crate::workflow::events::WorkflowEvent;
use crate::workflow::handler::{DispatchTable, ExecContext, StepOutcome};
use crate::workflow::OutputScope;
use serde::{Deserialize, Serialize};

/// Top-level status for the workflow run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Ready to advance.
    Running,
    /// Blocked on human approval of a specific gate.
    AwaitingApproval { gate: PendingGate },
    /// All slots consumed; no failures.
    Completed,
    /// A handler returned [`StepOutcome::Fail`]; run is halted.
    Failed {
        reason: String,
        at_step: Option<String>,
    },
    /// External cancellation invoked via [`WorkflowExecution::cancel`].
    Cancelled,
}

impl WorkflowStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkflowStatus::Completed | WorkflowStatus::Failed { .. } | WorkflowStatus::Cancelled
        )
    }

    pub fn is_awaiting_gate(&self, gate_id: &str) -> bool {
        matches!(self, WorkflowStatus::AwaitingApproval { gate } if gate.id == gate_id)
    }
}

/// Serializable subset of [`WorkflowExecution`] — the fields the
/// daemon persists to disk between RPC calls so it can resume a run
/// after a restart. Slot list and dispatch table are rebuilt on
/// rehydrate; pending events are already drained by the time a
/// snapshot is taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub doc: WorkflowDoc,
    pub cursor: usize,
    pub scope: OutputScope,
    pub status: WorkflowStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingGate {
    pub id: String,
    pub title: Option<String>,
    /// Step id that owns the gate, or `"preamble"` for workflow-level.
    pub owner: String,
}

/// One unit of execution. The engine pre-computes a flat list of these
/// from the WorkflowDoc so `tick()` is a simple index increment.
#[derive(Debug, Clone)]
enum Slot {
    PreambleGate {
        gate_id: String,
        gate: Gate,
    },
    StepGate {
        step_id: String,
        gate_id: String,
        gate: Gate,
    },
    Step {
        step_id: String,
        /// Path to the step, e.g. `[0, 1]` → `doc.steps[0].children[1]`.
        path: Vec<usize>,
    },
    /// Run of ≥2 consecutive parallel-marked siblings, one path per
    /// member. Each member is a branch: the member step plus its
    /// descendants run sequentially within the branch, branches run
    /// concurrently, and the engine joins all of them before advancing.
    ParallelGroup {
        members: Vec<Vec<usize>>,
    },
}

pub struct WorkflowExecution {
    doc: WorkflowDoc,
    slots: Vec<Slot>,
    cursor: usize,
    scope: OutputScope,
    status: WorkflowStatus,
    dispatch: DispatchTable,
    /// Events emitted since the last drain. The daemon drains after
    /// every `tick()` so subscribers see progress in order.
    pending_events: Vec<WorkflowEvent>,
}

impl WorkflowExecution {
    pub fn new(doc: WorkflowDoc, dispatch: DispatchTable) -> Self {
        let slots = flatten(&doc);
        Self {
            doc,
            slots,
            cursor: 0,
            scope: OutputScope::new(),
            status: WorkflowStatus::Running,
            dispatch,
            pending_events: Vec::new(),
        }
    }

    /// Take a durable snapshot of execution state — enough to rehydrate
    /// with the same [`WorkflowDoc`] and a freshly-built [`DispatchTable`].
    /// Slots are re-derived from `doc`, so we don't serialize them.
    /// `pending_events` is transient (the daemon drains after every
    /// tick).
    pub fn snapshot(&self) -> WorkflowSnapshot {
        WorkflowSnapshot {
            doc: self.doc.clone(),
            cursor: self.cursor,
            scope: self.scope.clone(),
            status: self.status.clone(),
        }
    }

    /// Rebuild an execution from a previously-captured snapshot. The
    /// caller supplies a fresh dispatch table (handlers hold runtime
    /// state that isn't — and shouldn't be — serialised).
    pub fn rehydrate(snapshot: WorkflowSnapshot, dispatch: DispatchTable) -> Self {
        let slots = flatten(&snapshot.doc);
        Self {
            doc: snapshot.doc,
            slots,
            cursor: snapshot.cursor,
            scope: snapshot.scope,
            status: snapshot.status,
            dispatch,
            pending_events: Vec::new(),
        }
    }

    pub fn doc(&self) -> &WorkflowDoc {
        &self.doc
    }

    pub fn status(&self) -> &WorkflowStatus {
        &self.status
    }

    pub fn scope(&self) -> &OutputScope {
        &self.scope
    }

    pub fn total_slots(&self) -> usize {
        self.slots.len()
    }

    pub fn completed_slots(&self) -> usize {
        self.cursor
    }

    /// Drain accumulated events. Daemon calls this after every `tick`
    /// to forward them onto the session event bus.
    pub fn drain_events(&mut self) -> Vec<WorkflowEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Advance one slot if `Running`. No-op if in any other state.
    pub async fn tick(&mut self) -> &WorkflowStatus {
        if !matches!(self.status, WorkflowStatus::Running) {
            return &self.status;
        }
        if self.cursor >= self.slots.len() {
            self.status = WorkflowStatus::Completed;
            self.pending_events.push(WorkflowEvent::WorkflowCompleted);
            return &self.status;
        }

        let slot = self.slots[self.cursor].clone();
        match slot {
            Slot::PreambleGate { gate_id, gate } => {
                self.status = WorkflowStatus::AwaitingApproval {
                    gate: PendingGate {
                        id: gate_id.clone(),
                        title: gate.title.clone(),
                        owner: "preamble".to_string(),
                    },
                };
                self.pending_events.push(WorkflowEvent::GateReached {
                    gate_id,
                    title: gate.title,
                    owner: "preamble".to_string(),
                });
            }
            Slot::StepGate {
                step_id,
                gate_id,
                gate,
            } => {
                self.status = WorkflowStatus::AwaitingApproval {
                    gate: PendingGate {
                        id: gate_id.clone(),
                        title: gate.title.clone(),
                        owner: step_id.clone(),
                    },
                };
                self.pending_events.push(WorkflowEvent::GateReached {
                    gate_id,
                    title: gate.title,
                    owner: step_id,
                });
            }
            Slot::Step { step_id, path } => {
                let step = resolve_step(&self.doc, &path).expect("slot path valid by construction");
                let step_type = step.attributes.get("type").map(|s| s.as_str());

                self.pending_events.push(WorkflowEvent::StepStarted {
                    step_id: step_id.clone(),
                    title: step.title.clone(),
                });

                let handler = self.dispatch.resolve(step_type);
                let ctx = ExecContext {
                    step,
                    step_id: &step_id,
                    scope: &self.scope,
                    validations: &self.doc.validations,
                };

                match handler.execute(&ctx).await {
                    StepOutcome::Advance { output } => {
                        let output_name = step.output.clone();
                        if let (Some(name), Some(value)) = (&step.output, output) {
                            self.scope.insert(name.clone(), value);
                        }
                        self.pending_events.push(WorkflowEvent::StepCompleted {
                            step_id,
                            output_name,
                        });
                        self.cursor += 1;
                        return &self.status;
                    }
                    StepOutcome::YieldForApproval {
                        gate_id,
                        gate_title,
                    } => {
                        // Handler opted to pause (e.g. the `gate` stdlib
                        // handler). Mark the step itself as awaiting
                        // approval; completion on approval advances past
                        // this slot without re-running the handler.
                        self.status = WorkflowStatus::AwaitingApproval {
                            gate: PendingGate {
                                id: gate_id.clone(),
                                title: gate_title.clone(),
                                owner: step_id.clone(),
                            },
                        };
                        self.pending_events.push(WorkflowEvent::GateReached {
                            gate_id,
                            title: gate_title,
                            owner: step_id,
                        });
                        return &self.status;
                    }
                    StepOutcome::Fail { reason } => {
                        self.pending_events.push(WorkflowEvent::WorkflowFailed {
                            reason: reason.clone(),
                            at_step: Some(step_id.clone()),
                        });
                        self.status = WorkflowStatus::Failed {
                            reason,
                            at_step: Some(step_id),
                        };
                        return &self.status;
                    }
                }
            }
            Slot::ParallelGroup { members } => {
                // Branches see the scope as of group start; sibling
                // branches have no ordering, so they can't observe each
                // other's outputs. Outputs and events merge in document
                // order after the join, keeping the stream deterministic
                // regardless of completion order.
                let results = futures::future::join_all(members.iter().map(|path| {
                    let root =
                        resolve_step(&self.doc, path).expect("slot path valid by construction");
                    run_branch(
                        root,
                        path.clone(),
                        self.scope.clone(),
                        &self.doc.validations,
                        &self.dispatch,
                    )
                }))
                .await;

                let mut failures: Vec<(String, String)> = Vec::new();
                for branch in results {
                    self.pending_events.extend(branch.events);
                    for (name, value) in branch.outputs {
                        self.scope.insert(name, value);
                    }
                    failures.extend(branch.failure);
                }

                if failures.is_empty() {
                    self.cursor += 1;
                    return &self.status;
                }
                // All branches ran to completion (or their own failure)
                // before we report — partial successes keep their events
                // and outputs.
                let reason = failures
                    .iter()
                    .map(|(id, r)| format!("{id}: {r}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                let at_step = Some(failures[0].0.clone());
                self.pending_events.push(WorkflowEvent::WorkflowFailed {
                    reason: reason.clone(),
                    at_step: at_step.clone(),
                });
                self.status = WorkflowStatus::Failed { reason, at_step };
                return &self.status;
            }
        }
        &self.status
    }

    /// Approve the currently-pending gate by id. Advances past the slot
    /// that blocked. Returns `Err` if no gate is pending or the id
    /// doesn't match.
    pub fn approve_gate(&mut self, gate_id: &str) -> Result<&WorkflowStatus, GateError> {
        match &self.status {
            WorkflowStatus::AwaitingApproval { gate } if gate.id == gate_id => {
                self.pending_events.push(WorkflowEvent::GateApproved {
                    gate_id: gate.id.clone(),
                });
                self.cursor += 1;
                self.status = WorkflowStatus::Running;
                Ok(&self.status)
            }
            WorkflowStatus::AwaitingApproval { gate } => Err(GateError::Mismatch {
                expected: gate.id.clone(),
                got: gate_id.to_string(),
            }),
            _ => Err(GateError::NoGatePending),
        }
    }

    /// Cancel execution immediately. No-op if already terminal.
    pub fn cancel(&mut self) {
        if self.status.is_terminal() {
            return;
        }
        self.status = WorkflowStatus::Cancelled;
        self.pending_events.push(WorkflowEvent::WorkflowCancelled);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("no gate currently pending")]
    NoGatePending,
    #[error("gate mismatch: expected '{expected}', got '{got}'")]
    Mismatch { expected: String, got: String },
}

// ---------- slot construction ----------

fn flatten(doc: &WorkflowDoc) -> Vec<Slot> {
    let mut slots = Vec::new();

    for (i, gate) in doc.preamble_gates.iter().enumerate() {
        slots.push(Slot::PreambleGate {
            gate_id: format!("preamble.gate{}", i),
            gate: gate.clone(),
        });
    }

    flatten_siblings(&doc.steps, &mut Vec::new(), &mut slots);

    slots
}

/// Walk a sibling list, collapsing runs of ≥2 consecutive parallel-marked
/// steps into one [`Slot::ParallelGroup`]. A lone parallel step degrades
/// to plain sequential flattening — there is nothing to overlap with.
fn flatten_siblings(steps: &[WorkflowStep], path: &mut Vec<usize>, out: &mut Vec<Slot>) {
    let mut i = 0;
    while i < steps.len() {
        let run = steps[i..].iter().take_while(|s| s.parallel).count();
        if run >= 2 {
            let members = (i..i + run)
                .map(|j| {
                    let mut member = path.clone();
                    member.push(j);
                    member
                })
                .collect();
            out.push(Slot::ParallelGroup { members });
            i += run;
        } else {
            path.push(i);
            flatten_step(&steps[i], path, out);
            path.pop();
            i += 1;
        }
    }
}

fn flatten_step(step: &WorkflowStep, path: &mut Vec<usize>, out: &mut Vec<Slot>) {
    let step_id = path_to_id(path);

    for (i, gate) in step.gates.iter().enumerate() {
        out.push(Slot::StepGate {
            step_id: step_id.clone(),
            gate_id: format!("{}.gate{}", step_id, i),
            gate: gate.clone(),
        });
    }
    out.push(Slot::Step {
        step_id,
        path: path.clone(),
    });

    flatten_siblings(&step.children, path, out);
}

/// What one parallel branch produced. Events and outputs are kept
/// branch-local during the run and merged by the engine after the join.
struct BranchOutcome {
    events: Vec<WorkflowEvent>,
    outputs: Vec<(String, serde_json::Value)>,
    /// `(step_id, reason)` — set when the branch halted early.
    failure: Option<(String, String)>,
}

/// Execute one parallel branch: the member step, then its descendants,
/// depth-first and strictly sequential within the branch. Later branch
/// steps see earlier branch outputs via the branch-local scope clone.
///
/// Gates can't pause a half-joined group (the cursor is slot-granular),
/// so a gate callout or a `YieldForApproval` outcome fails the branch
/// instead.
async fn run_branch(
    root: &WorkflowStep,
    root_path: Vec<usize>,
    mut scope: OutputScope,
    validations: &[crate::parser::types::ValidationEntry],
    dispatch: &DispatchTable,
) -> BranchOutcome {
    let mut out = BranchOutcome {
        events: Vec::new(),
        outputs: Vec::new(),
        failure: None,
    };

    let mut stack: Vec<(&WorkflowStep, Vec<usize>)> = vec![(root, root_path)];
    while let Some((step, path)) = stack.pop() {
        let step_id = path_to_id(&path);

        if !step.gates.is_empty() {
            out.failure = Some((
                step_id,
                "human gates are not supported inside parallel groups".to_string(),
            ));
            return out;
        }

        out.events.push(WorkflowEvent::StepStarted {
            step_id: step_id.clone(),
            title: step.title.clone(),
        });

        let handler = dispatch.resolve(step.attributes.get("type").map(|s| s.as_str()));
        let ctx = ExecContext {
            step,
            step_id: &step_id,
            scope: &scope,
            validations,
        };
        match handler.execute(&ctx).await {
            StepOutcome::Advance { output } => {
                let output_name = step.output.clone();
                if let (Some(name), Some(value)) = (&step.output, output) {
                    scope.insert(name.clone(), value.clone());
                    out.outputs.push((name.clone(), value));
                }
                out.events.push(WorkflowEvent::StepCompleted {
                    step_id,
                    output_name,
                });
            }
            StepOutcome::YieldForApproval { .. } => {
                out.failure = Some((
                    step_id,
                    "human gates are not supported inside parallel groups".to_string(),
                ));
                return out;
            }
            StepOutcome::Fail { reason } => {
                out.failure = Some((step_id, reason));
                return out;
            }
        }

        for (i, child) in step.children.iter().enumerate().rev() {
            let mut child_path = path.clone();
            child_path.push(i);
            stack.push((child, child_path));
        }
    }
    out
}

fn path_to_id(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

fn resolve_step<'a>(doc: &'a WorkflowDoc, path: &[usize]) -> Option<&'a WorkflowStep> {
    let (first, rest) = path.split_first()?;
    let mut cur = doc.steps.get(*first)?;
    for &idx in rest {
        cur = cur.children.get(idx)?;
    }
    Some(cur)
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::types::{Frontmatter, FrontmatterFormat, ParsedNote, WorkflowDoc};
    use crate::workflow::handler::StepHandler;
    use crate::workflow::stdlib::stdlib_dispatch;
    use std::path::PathBuf;

    fn exec_from(source: &str) -> WorkflowExecution {
        let (fm, _) = split_frontmatter(source);
        let mut note = ParsedNote::new(PathBuf::from("test.md"));
        note.frontmatter = fm;
        let doc = WorkflowDoc::from_parsed(&note, source).expect("workflow");
        WorkflowExecution::new(doc, stdlib_dispatch())
    }

    fn split_frontmatter(source: &str) -> (Option<Frontmatter>, String) {
        if let Some(rest) = source.strip_prefix("---\n") {
            if let Some(end) = rest.find("\n---\n") {
                return (
                    Some(Frontmatter::new(
                        rest[..end].to_string(),
                        FrontmatterFormat::Yaml,
                    )),
                    rest[end + "\n---\n".len()..].to_string(),
                );
            }
        }
        (None, source.to_string())
    }

    async fn run_until_gate_or_done(exec: &mut WorkflowExecution) -> Vec<WorkflowEvent> {
        let mut events = Vec::new();
        loop {
            exec.tick().await;
            events.extend(exec.drain_events());
            if !matches!(exec.status(), WorkflowStatus::Running) {
                return events;
            }
        }
    }

    #[tokio::test]
    async fn linear_workflow_runs_to_completion() {
        let source = "\
---
type: workflow
---
## Plan
## Build
## Ship
";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        // Three StepStarted + three StepCompleted + WorkflowCompleted.
        assert_eq!(events.len(), 7);
        assert!(matches!(events[0], WorkflowEvent::StepStarted { .. }));
        assert!(matches!(
            events.last(),
            Some(WorkflowEvent::WorkflowCompleted)
        ));
    }

    #[tokio::test]
    async fn step_with_output_binds_scope() {
        let source = "\
---
type: workflow
---
## Parse -> config
## Use
";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        assert!(exec.scope().contains_key("config"));
    }

    #[tokio::test]
    async fn gate_pauses_and_approval_resumes() {
        let source = "\
---
type: workflow
---
## Approve [type:: gate]
Require sign-off.
## Do Thing
";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::AwaitingApproval { gate } = exec.status().clone() else {
            panic!("expected AwaitingApproval, got {:?}", exec.status());
        };
        // After approval, completes.
        exec.approve_gate(&gate.id).unwrap();
        let events = run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::GateApproved { .. })));
    }

    #[tokio::test]
    async fn preamble_gate_blocks_before_any_step() {
        let source = "\
---
type: workflow
---
> [!gate]
> Leadership approval

## Do Thing
";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::AwaitingApproval { gate } = exec.status().clone() else {
            panic!();
        };
        assert_eq!(gate.owner, "preamble");
        // No StepStarted should have been emitted yet.
        assert!(!events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::StepStarted { .. })));
        exec.approve_gate(&gate.id).unwrap();
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn step_level_gate_fires_before_step_body() {
        let source = "\
---
type: workflow
---
## Deploy
> [!gate]
> Require approval
## After
";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::AwaitingApproval { gate } = exec.status().clone() else {
            panic!();
        };
        assert_eq!(gate.owner, "0");
        // The Deploy step has not yet started.
        assert!(!events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::StepStarted { .. })));
    }

    #[tokio::test]
    async fn nested_children_run_in_dfs_order() {
        let source = "\
---
type: workflow
---
## A
### A1
### A2
## B
";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        let titles: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::StepStarted { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(titles, vec!["A", "A1", "A2", "B"]);
    }

    #[tokio::test]
    async fn approve_with_wrong_id_errors() {
        let source = "---\ntype: workflow\n---\n## G [type:: gate]\n";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        let err = exec.approve_gate("wrong").unwrap_err();
        assert!(matches!(err, GateError::Mismatch { .. }));
    }

    #[test]
    fn approve_when_no_gate_pending_errors() {
        let source = "---\ntype: workflow\n---\n## X\n";
        let mut exec = exec_from(source);
        let err = exec.approve_gate("any").unwrap_err();
        assert!(matches!(err, GateError::NoGatePending));
    }

    #[tokio::test]
    async fn cancel_transitions_to_cancelled() {
        let source = "---\ntype: workflow\n---\n## A\n## B\n";
        let mut exec = exec_from(source);
        exec.tick().await;
        let _ = exec.drain_events();
        exec.cancel();
        assert_eq!(exec.status(), &WorkflowStatus::Cancelled);
        let events = exec.drain_events();
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::WorkflowCancelled)));
        // Subsequent ticks are no-ops.
        exec.tick().await;
        assert_eq!(exec.status(), &WorkflowStatus::Cancelled);
    }

    #[tokio::test]
    async fn cancel_after_terminal_is_noop() {
        let source = "---\ntype: workflow\n---\n## A\n";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        exec.cancel();
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn unknown_step_type_falls_back_to_default() {
        let source = "---\ntype: workflow\n---\n## Custom [type:: unknown-type]\n";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        // Unknown types route to default, which advances successfully.
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn snapshot_roundtrip_preserves_cursor_and_scope() {
        let source = "\
---
type: workflow
---
## First -> first_out
## Second [type:: gate]
## Third
";
        let mut exec = exec_from(source);
        // Run to the gate — cursor past first step, scope populated.
        run_until_gate_or_done(&mut exec).await;
        assert!(matches!(
            exec.status(),
            WorkflowStatus::AwaitingApproval { .. }
        ));
        assert!(exec.scope().contains_key("first_out"));

        // Round-trip through serialization.
        let snapshot = exec.snapshot();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let snapshot2: WorkflowSnapshot = serde_json::from_slice(&bytes).unwrap();

        let mut rehydrated = WorkflowExecution::rehydrate(snapshot2, stdlib_dispatch());
        assert_eq!(rehydrated.status(), exec.status());
        assert_eq!(rehydrated.completed_slots(), exec.completed_slots());
        assert!(rehydrated.scope().contains_key("first_out"));

        // Approve the gate on the rehydrated execution — it should
        // progress as if it had been running all along.
        let WorkflowStatus::AwaitingApproval { gate } = rehydrated.status().clone() else {
            panic!();
        };
        rehydrated.approve_gate(&gate.id).unwrap();
        run_until_gate_or_done(&mut rehydrated).await;
        assert_eq!(rehydrated.status(), &WorkflowStatus::Completed);
    }

    fn started_titles(events: &[WorkflowEvent]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::StepStarted { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Fails unless every member of the group reaches the barrier — i.e.
    /// the engine really overlaps their execution.
    struct BarrierHandler(std::sync::Arc<tokio::sync::Barrier>);

    #[async_trait::async_trait]
    impl StepHandler for BarrierHandler {
        async fn execute(&self, _ctx: &ExecContext<'_>) -> StepOutcome {
            match tokio::time::timeout(std::time::Duration::from_secs(5), self.0.wait()).await {
                Ok(_) => StepOutcome::Advance { output: None },
                Err(_) => StepOutcome::Fail {
                    reason: "barrier timeout: members did not run concurrently".into(),
                },
            }
        }
    }

    struct AlwaysFail;

    #[async_trait::async_trait]
    impl StepHandler for AlwaysFail {
        async fn execute(&self, _ctx: &ExecContext<'_>) -> StepOutcome {
            StepOutcome::Fail {
                reason: "intentional".into(),
            }
        }
    }

    fn exec_with_table(source: &str, table: DispatchTable) -> WorkflowExecution {
        let (fm, _) = split_frontmatter(source);
        let mut note = ParsedNote::new(PathBuf::from("test.md"));
        note.frontmatter = fm;
        let doc = WorkflowDoc::from_parsed(&note, source).expect("workflow");
        WorkflowExecution::new(doc, table)
    }

    #[tokio::test]
    async fn consecutive_parallel_steps_run_as_one_group_then_next_waits() {
        let source = "---\ntype: workflow\n---\n## &A\n## &B\n## C\n";
        let mut exec = exec_from(source);
        assert_eq!(
            exec.total_slots(),
            2,
            "parallel pair collapses into one slot"
        );
        let events = run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        assert_eq!(started_titles(&events), vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn parallel_group_emits_start_and_complete_per_member_in_doc_order() {
        let source = "---\ntype: workflow\n---\n## &A\n## &B\n";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        let kinds: Vec<String> = events
            .iter()
            .map(|e| match e {
                WorkflowEvent::StepStarted { title, .. } => format!("start {title}"),
                WorkflowEvent::StepCompleted { step_id, .. } => format!("done {step_id}"),
                WorkflowEvent::WorkflowCompleted => "completed".to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["start A", "done 0", "start B", "done 1", "completed"]
        );
    }

    #[tokio::test]
    async fn parallel_section_children_run_concurrently() {
        let source = "\
---
type: workflow
---
## Build (parallel)
### A [type:: barrier]
### B [type:: barrier]
";
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let mut table = stdlib_dispatch();
        table.register("barrier", Box::new(BarrierHandler(barrier)));
        let mut exec = exec_with_table(source, table);
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn parallel_member_outputs_bind_scope_after_join() {
        let source = "---\ntype: workflow\n---\n## &A -> out_a\n## &B -> out_b\n## C\n";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        assert!(exec.scope().contains_key("out_a"));
        assert!(exec.scope().contains_key("out_b"));
    }

    #[tokio::test]
    async fn failing_parallel_member_fails_workflow_after_join_reporting_all_failures() {
        let source = "\
---
type: workflow
---
## &X [type:: boom]
## &Y [type:: boom]
## &Z
## After
";
        let mut table = stdlib_dispatch();
        table.register("boom", Box::new(AlwaysFail));
        let mut exec = exec_with_table(source, table);
        let events = run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::Failed { reason, at_step } = exec.status() else {
            panic!("expected Failed, got {:?}", exec.status());
        };
        assert!(reason.contains("0:"), "first failure reported: {reason}");
        assert!(reason.contains("1:"), "second failure reported: {reason}");
        assert_eq!(at_step.as_deref(), Some("0"));
        // The healthy member still ran to completion before the join.
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::StepCompleted { step_id, .. } if step_id == "2")));
        // Nothing after the group starts.
        assert!(!started_titles(&events).contains(&"After"));
    }

    #[tokio::test]
    async fn gate_step_inside_parallel_group_fails_that_branch() {
        let source = "---\ntype: workflow\n---\n## &G [type:: gate]\n## &H\n";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::Failed { reason, .. } = exec.status() else {
            panic!("expected Failed, got {:?}", exec.status());
        };
        assert!(reason.contains("gate"), "reason mentions gates: {reason}");
        assert!(events
            .iter()
            .any(|e| matches!(e, WorkflowEvent::StepCompleted { step_id, .. } if step_id == "1")));
    }

    #[tokio::test]
    async fn gate_callout_inside_parallel_member_fails_branch() {
        let source = "\
---
type: workflow
---
## &D

> [!gate]
> Needs approval

## &E
";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::Failed { reason, at_step } = exec.status() else {
            panic!("expected Failed, got {:?}", exec.status());
        };
        assert!(reason.contains("gate"), "reason mentions gates: {reason}");
        assert_eq!(at_step.as_deref(), Some("0"));
    }

    #[tokio::test]
    async fn single_parallel_step_runs_sequentially() {
        let source = "---\ntype: workflow\n---\n## &Only\n## Next\n";
        let mut exec = exec_from(source);
        assert_eq!(exec.total_slots(), 2, "run of one parallel step is plain");
        run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn parallel_member_children_run_sequentially_within_branch() {
        let source = "\
---
type: workflow
---
## Build (parallel)
### A
#### A1
### B
";
        let mut exec = exec_from(source);
        let events = run_until_gate_or_done(&mut exec).await;
        assert_eq!(exec.status(), &WorkflowStatus::Completed);
        let titles = started_titles(&events);
        // A's subtree stays ordered within its branch; branches merge in
        // document order after the join.
        assert_eq!(titles, vec!["Build", "A", "A1", "B"]);
    }

    #[tokio::test]
    async fn snapshot_roundtrip_preserves_parallel_group_position() {
        let source = "---\ntype: workflow\n---\n## First [type:: gate]\n## &A -> out_a\n## &B\n";
        let mut exec = exec_from(source);
        run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::AwaitingApproval { gate } = exec.status().clone() else {
            panic!("expected gate, got {:?}", exec.status());
        };

        let bytes = serde_json::to_vec(&exec.snapshot()).unwrap();
        let snapshot: WorkflowSnapshot = serde_json::from_slice(&bytes).unwrap();
        let mut rehydrated = WorkflowExecution::rehydrate(snapshot, stdlib_dispatch());
        assert_eq!(rehydrated.total_slots(), exec.total_slots());

        rehydrated.approve_gate(&gate.id).unwrap();
        run_until_gate_or_done(&mut rehydrated).await;
        assert_eq!(rehydrated.status(), &WorkflowStatus::Completed);
        assert!(rehydrated.scope().contains_key("out_a"));
    }

    #[tokio::test]
    async fn failed_handler_halts_workflow() {
        struct AlwaysFail;
        #[async_trait::async_trait]
        impl crate::workflow::handler::StepHandler for AlwaysFail {
            async fn execute(&self, _ctx: &ExecContext<'_>) -> StepOutcome {
                StepOutcome::Fail {
                    reason: "intentional".into(),
                }
            }
        }
        let source = "---\ntype: workflow\n---\n## X [type:: boom]\n## Y\n";
        let (fm, _) = split_frontmatter(source);
        let mut note = ParsedNote::new(PathBuf::from("test.md"));
        note.frontmatter = fm;
        let doc = WorkflowDoc::from_parsed(&note, source).unwrap();
        let mut table = stdlib_dispatch();
        table.register("boom", Box::new(AlwaysFail));
        let mut exec = WorkflowExecution::new(doc, table);
        run_until_gate_or_done(&mut exec).await;
        let WorkflowStatus::Failed { reason, at_step } = exec.status() else {
            panic!("expected Failed, got {:?}", exec.status());
        };
        assert_eq!(reason, "intentional");
        assert_eq!(at_step.as_deref(), Some("0"));
    }
}
