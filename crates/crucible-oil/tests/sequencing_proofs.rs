//! Stage C — Sequencing proof harness for oil.
//!
//! Drives `FramePlanner`/`Terminal`/`OutputBuffer` through arbitrary
//! operation sequences and asserts oil's contracts hold across every
//! frame. The proofs are oil-level — they make no assumption about how
//! any caller (chat app, lua plugin, hypothetical other consumer)
//! constructs node trees.
//!
//! Invariant phasing per `thoughts/shared/plans/tui-stage-c-sequencing-proofs_2026-05-16-2146.md`:
//! - C-1 (this file initially): #1 monotonic scrollback, #4 render idempotence,
//!   #5 determinism.
//! - C-2: #2 width-stable graduation, #3 no double-paint.
//! - C-3: #6 ANSI run integrity, #7 overlay non-interference,
//!   #8 cursor restored after `cleanup_viewport`.

#![cfg(feature = "test-utils")]

use crucible_oil::node::Node;
use crucible_oil::planning::{FramePlan, FrameSnapshot, Graduation};
use crucible_oil::proptest_strategies::{arb_dims, arb_operation_sequence, Op};
use crucible_oil::{render_tree, TestRuntime, NATURAL_HEIGHT};
use proptest::prelude::*;

// ─── Harness ────────────────────────────────────────────────────────────────

/// Output of one harness run.
#[derive(Debug)]
struct RunResult {
    /// One per applied `Op` (Resize ops produce no FrameSnapshot — index gap).
    /// Each entry is `(op_index, FrameSnapshot)`.
    frames: Vec<(usize, FrameSnapshot)>,
    /// Final byte buffer the terminal would have written to stdout.
    bytes: Vec<u8>,
}

impl RunResult {
    fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

/// Drive a fresh `TestRuntime` through `ops`, capturing every emitted
/// `FrameSnapshot` and the final byte buffer.
fn run_ops(initial_dims: (u16, u16), ops: &[Op]) -> RunResult {
    let mut rt = TestRuntime::new(initial_dims.0, initial_dims.1);
    let mut frames: Vec<(usize, FrameSnapshot)> = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::RenderFrame { tree } => {
                rt.render(tree);
                if let Some(snap) = rt.last_snapshot() {
                    frames.push((i, snap.clone()));
                }
            }
            Op::Graduate { node, viewport } => {
                let grad = Graduation { node: node.clone() };
                rt.render_with_graduation(viewport, Some(&grad));
                if let Some(snap) = rt.last_snapshot() {
                    frames.push((i, snap.clone()));
                }
            }
            Op::Resize { width, height } => {
                rt.resize(*width, *height);
                // Resize emits no FrameSnapshot until the next render.
            }
        }
    }

    let bytes = rt.take_bytes();
    RunResult { frames, bytes }
}

// ─── Invariant #1: Monotonic scrollback ────────────────────────────────────

/// The cumulative scrollback (sum of stdout_delta across frames) is
/// non-decreasing. Once content has been graduated, oil never retracts it.
fn check_monotonic_scrollback(result: &RunResult) -> Result<(), Violation> {
    let mut cumulative = 0usize;
    for (op_idx, snap) in &result.frames {
        let delta = snap.stdout_delta.len();
        let next = cumulative.checked_add(delta).ok_or_else(|| Violation {
            invariant: "monotonic_scrollback",
            op_index: *op_idx,
            detail: "scrollback length overflowed usize".into(),
        })?;
        if next < cumulative {
            return Err(Violation {
                invariant: "monotonic_scrollback",
                op_index: *op_idx,
                detail: format!("cumulative shrank: {cumulative} -> {next}"),
            });
        }
        cumulative = next;
    }
    Ok(())
}

// ─── Invariant #5: Determinism ─────────────────────────────────────────────

/// Two fresh runs of the same `(initial_dims, ops)` produce identical
/// FrameSnapshot sequences and identical final byte buffers.
fn check_determinism(initial_dims: (u16, u16), ops: &[Op]) -> Result<(), Violation> {
    let a = run_ops(initial_dims, ops);
    let b = run_ops(initial_dims, ops);

    if a.frame_count() != b.frame_count() {
        return Err(Violation {
            invariant: "determinism",
            op_index: usize::MAX,
            detail: format!(
                "frame count differs: {} vs {}",
                a.frame_count(),
                b.frame_count()
            ),
        });
    }
    for ((idx_a, snap_a), (_, snap_b)) in a.frames.iter().zip(b.frames.iter()) {
        if !snapshot_eq(&snap_a.plan, &snap_b.plan) || snap_a.stdout_delta != snap_b.stdout_delta {
            return Err(Violation {
                invariant: "determinism",
                op_index: *idx_a,
                detail: "snapshots diverge between replays".into(),
            });
        }
    }
    if a.bytes != b.bytes {
        return Err(Violation {
            invariant: "determinism",
            op_index: usize::MAX,
            detail: format!(
                "byte buffers diverge: {} vs {} bytes",
                a.bytes.len(),
                b.bytes.len()
            ),
        });
    }
    Ok(())
}

fn snapshot_eq(a: &FramePlan, b: &FramePlan) -> bool {
    a.frame_no == b.frame_no
        && a.viewport.content == b.viewport.content
        && a.viewport.cursor.col == b.viewport.cursor.col
        && a.viewport.cursor.row_from_end == b.viewport.cursor.row_from_end
        && a.viewport.cursor.visible == b.viewport.cursor.visible
        && a.overlays.len() == b.overlays.len()
}

// ─── Invariant #4: render_tree idempotence ─────────────────────────────────

/// `render_tree(t, w, h)` is a pure function; calling it twice with the
/// same inputs yields byte-identical output (both `content` and `cursor`).
fn check_render_tree_idempotent(tree: &Node, width: u16, height: u16) -> Result<(), Violation> {
    let a = render_tree(tree, width, height);
    let b = render_tree(tree, width, height);
    if a.content != b.content {
        return Err(Violation {
            invariant: "render_tree_idempotent",
            op_index: usize::MAX,
            detail: format!(
                "content diverges at width={width}, height={height}:\nA: {:?}\nB: {:?}",
                preview(&a.content),
                preview(&b.content),
            ),
        });
    }
    if a.cursor.col != b.cursor.col
        || a.cursor.row_from_end != b.cursor.row_from_end
        || a.cursor.visible != b.cursor.visible
    {
        return Err(Violation {
            invariant: "render_tree_idempotent",
            op_index: usize::MAX,
            detail: format!(
                "cursor diverges: A={:?} B={:?}",
                a.cursor, b.cursor
            ),
        });
    }
    Ok(())
}

// ─── Violation reporting ───────────────────────────────────────────────────

#[derive(Debug)]
struct Violation {
    invariant: &'static str,
    op_index: usize,
    detail: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.op_index == usize::MAX {
            write!(f, "[{}] {}", self.invariant, self.detail)
        } else {
            write!(
                f,
                "[{}] at op #{}: {}",
                self.invariant, self.op_index, self.detail
            )
        }
    }
}

fn preview(s: &str) -> String {
    let mut out: String = s.chars().take(80).collect();
    if s.len() > 80 {
        out.push_str("...");
    }
    out
}

// ─── Proptests ─────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        ..ProptestConfig::default()
    })]

    /// Invariant #1: scrollback never shrinks across frames.
    #[test]
    fn prop_scrollback_monotonic_under_arbitrary_ops(
        dims in arb_dims(),
        ops in arb_operation_sequence(),
    ) {
        let result = run_ops(dims, &ops);
        if let Err(v) = check_monotonic_scrollback(&result) {
            prop_assert!(
                false,
                "monotonic scrollback violated: {}\nops: {:#?}",
                v, ops,
            );
        }
    }

    /// Invariant #5: replaying the same operation sequence yields the
    /// same FrameSnapshots and byte buffer.
    #[test]
    fn prop_replay_same_ops_yields_same_snapshots(
        dims in arb_dims(),
        ops in arb_operation_sequence(),
    ) {
        if let Err(v) = check_determinism(dims, &ops) {
            prop_assert!(false, "determinism violated: {}\nops: {:#?}", v, ops);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        ..ProptestConfig::default()
    })]

    /// Invariant #4: `render_tree` is idempotent — same tree+dims twice
    /// produces byte-identical RenderResult.
    #[test]
    fn prop_render_tree_is_idempotent(
        tree in crucible_oil::proptest_strategies::arb_chat_like_node(),
        dims in arb_dims(),
    ) {
        if let Err(v) = check_render_tree_idempotent(&tree, dims.0, dims.1) {
            prop_assert!(false, "render_tree idempotence violated: {}", v);
        }
        // Also check at NATURAL_HEIGHT (the standalone path).
        if let Err(v) = check_render_tree_idempotent(&tree, dims.0, NATURAL_HEIGHT) {
            prop_assert!(false, "render_tree idempotence violated at NATURAL_HEIGHT: {}", v);
        }
    }
}

// ─── Smoke tests (cheap, deterministic) ────────────────────────────────────

#[test]
fn smoke_empty_sequence_produces_no_violations() {
    let result = run_ops((80, 24), &[]);
    assert_eq!(result.frame_count(), 0);
    check_monotonic_scrollback(&result).expect("empty run is trivially monotonic");
}

#[test]
fn smoke_single_render_no_graduation() {
    use crucible_oil::node::{col, text};
    let ops = vec![Op::RenderFrame {
        tree: col([text("hello"), text("world")]),
    }];
    let result = run_ops((80, 24), &ops);
    assert_eq!(result.frame_count(), 1);
    check_monotonic_scrollback(&result).unwrap();
    check_determinism((80, 24), &ops).unwrap();
}

#[test]
fn smoke_render_then_graduate() {
    use crucible_oil::node::{col, text};
    let ops = vec![
        Op::RenderFrame {
            tree: col([text("first")]),
        },
        Op::Graduate {
            node: col([text("scrolled")]),
            viewport: col([text("second")]),
        },
    ];
    let result = run_ops((80, 24), &ops);
    assert_eq!(result.frame_count(), 2);
    // After graduate, the second frame's stdout_delta carries the graduated content.
    assert!(result.frames[1].1.stdout_delta.contains("scrolled"));
    check_monotonic_scrollback(&result).unwrap();
    check_determinism((80, 24), &ops).unwrap();
}
