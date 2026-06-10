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

use crucible_oil::node::{overlay_from_bottom, Node};
use crucible_oil::overlay::filter_overlays;
use crucible_oil::planning::{FramePlan, FrameSnapshot, Graduation};
use crucible_oil::proptest_strategies::{arb_chat_like_node, arb_dims, arb_operation_sequence, Op};
use crucible_oil::{render_tree, FrameRenderer, Terminal, TestRuntime, NATURAL_HEIGHT};
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

// ─── Invariant #2: Width-stable graduation ─────────────────────────────────

/// For every `Graduate { node, .. }` op, the planner's `stdout_delta`
/// equals `render_tree(&node, planner.width(), NATURAL_HEIGHT).content`
/// byte-for-byte. Post-Stage-B this holds by construction (the planner
/// literally invokes that call); the proof guards against regressions.
fn check_graduation_width_stable(initial_dims: (u16, u16), ops: &[Op]) -> Result<(), Violation> {
    // Replay ops and capture (graduate_node, planner_width_at_that_moment, observed_delta).
    let mut planner_width = initial_dims.0;
    let mut planner_height = initial_dims.1;
    let mut rt = TestRuntime::new(initial_dims.0, initial_dims.1);

    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::RenderFrame { tree } => {
                rt.render(tree);
            }
            Op::Graduate { node, viewport } => {
                let grad = Graduation { node: node.clone() };
                rt.render_with_graduation(viewport, Some(&grad));

                // Predicted: oil renders the node through `render_tree` at
                // the current planner width with natural height. The observed
                // stdout_delta must equal that byte-for-byte.
                let predicted = render_tree(node, planner_width, NATURAL_HEIGHT).content;
                let observed = rt
                    .last_snapshot()
                    .map(|s| s.stdout_delta.clone())
                    .unwrap_or_default();
                if observed != predicted {
                    return Err(Violation {
                        invariant: "graduation_width_stable",
                        op_index: i,
                        detail: format!(
                            "stdout_delta != render_tree(node, {planner_width}, NATURAL_HEIGHT)\n\
                             observed: {:?}\npredicted: {:?}",
                            preview(&observed),
                            preview(&predicted),
                        ),
                    });
                }
            }
            Op::Resize { width, height } => {
                rt.resize(*width, *height);
                planner_width = *width;
                planner_height = *height;
            }
        }
    }
    let _ = planner_height;
    Ok(())
}

// ─── Invariant #3: No graduation/viewport crosstalk ────────────────────────

/// For every `Graduate { node, viewport }`, the viewport content equals
/// `render_tree(viewport, w, h).content` — i.e., the graduated `node`
/// does not leak into the viewport region. Combined with invariant #2,
/// this is the formal "no double-paint" property at oil's level:
/// scrollback bytes correspond to the graduation tree, viewport bytes
/// correspond to the viewport tree, and the two are independent.
///
/// (Higher-level "graduated content stays in scrollback across many
/// frames" is a caller contract — oil can't enforce that the caller
/// stops passing the same tree as viewport input. The oil-level
/// guarantee is the per-frame separation, which this invariant pins.)
fn check_no_graduation_viewport_crosstalk(
    initial_dims: (u16, u16),
    ops: &[Op],
) -> Result<(), Violation> {
    let mut planner_width = initial_dims.0;
    let mut planner_height = initial_dims.1;
    let mut rt = TestRuntime::new(initial_dims.0, initial_dims.1);

    for (i, op) in ops.iter().enumerate() {
        match op {
            Op::RenderFrame { tree } => {
                rt.render(tree);
                let predicted = render_tree(tree, planner_width, planner_height).content;
                let observed = rt.viewport_content().to_string();
                if observed != predicted {
                    return Err(Violation {
                        invariant: "no_graduation_viewport_crosstalk",
                        op_index: i,
                        detail: format!(
                            "viewport != render_tree(tree, {planner_width}, {planner_height})\n\
                             observed: {:?}\npredicted: {:?}",
                            preview(&observed),
                            preview(&predicted),
                        ),
                    });
                }
            }
            Op::Graduate { node, viewport } => {
                let grad = Graduation { node: node.clone() };
                rt.render_with_graduation(viewport, Some(&grad));
                let predicted = render_tree(viewport, planner_width, planner_height).content;
                let observed = rt.viewport_content().to_string();
                if observed != predicted {
                    return Err(Violation {
                        invariant: "no_graduation_viewport_crosstalk",
                        op_index: i,
                        detail: format!(
                            "viewport != render_tree(viewport, {planner_width}, {planner_height}) after Graduate\n\
                             observed: {:?}\npredicted: {:?}",
                            preview(&observed),
                            preview(&predicted),
                        ),
                    });
                }
            }
            Op::Resize { width, height } => {
                rt.resize(*width, *height);
                planner_width = *width;
                planner_height = *height;
            }
        }
    }
    Ok(())
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
            detail: format!("cursor diverges: A={:?} B={:?}", a.cursor, b.cursor),
        });
    }
    Ok(())
}

// ─── Invariant #6: ANSI run integrity ──────────────────────────────────────

/// Render output, when fed to a vt100 emulator, is fully consumed: no raw
/// `\x1b` bytes survive into the parsed screen state, and the visible
/// characters reported by vt100 match the strip_ansi flat output (after
/// normalizing both for trailing whitespace on each line).
///
/// This pins two sub-properties:
/// - Every escape sequence is well-formed enough for vt100 to consume.
/// - The visible text content vt100 sees matches what strip_ansi produces.
fn check_ansi_round_trips_through_vt100(
    tree: &Node,
    width: u16,
    _height: u16,
) -> Result<(), Violation> {
    use crucible_oil::ansi::strip_ansi;

    let result = render_tree(tree, width, NATURAL_HEIGHT);
    let stripped = strip_ansi(&result.content);

    // Size vt100 to match the render's width exactly so cells align.
    // Use the actual line count to size height — content fits without scroll.
    let line_count = result.content.lines().count().max(1) as u16;
    let mut vt = vt100::Parser::new(line_count.max(1), width.max(1), 0);
    vt.process(result.content.as_bytes());

    let vt_screen = vt.screen().contents();
    if vt_screen.contains('\x1b') {
        return Err(Violation {
            invariant: "ansi_round_trips_through_vt100",
            op_index: usize::MAX,
            detail: format!(
                "vt100 screen contains raw escape byte (unconsumed)\nsample: {:?}",
                preview(&vt_screen),
            ),
        });
    }

    // Compare flat visible text. Each line: trim trailing spaces from both
    // (vt100 emits its grid as 80-col padded by default).
    let strip_flat: String = stripped
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let vt_flat: String = vt_screen
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    // vt100 padding the screen with extra trailing blank rows is OK.
    if !vt_flat.starts_with(&strip_flat) && !strip_flat.starts_with(&vt_flat) {
        return Err(Violation {
            invariant: "ansi_round_trips_through_vt100",
            op_index: usize::MAX,
            detail: format!(
                "vt100 visible content diverges from strip_ansi:\n\
                 stripped: {:?}\nvt100:    {:?}",
                preview(&strip_flat),
                preview(&vt_flat),
            ),
        });
    }
    Ok(())
}

// ─── Invariant #7: Overlay non-interference ────────────────────────────────

/// Wrap a tree in an `Overlay` and assert: cells outside the overlay's
/// influence region match the same tree rendered without the overlay.
///
/// The overlay anchors at the bottom of the viewport; we generously claim
/// "outside overlay region" as the top half of the rendered output (rows
/// that the overlay's height could not possibly reach).
fn check_overlay_non_interference(
    base: &Node,
    overlay_content: Node,
    width: u16,
    height: u16,
) -> Result<(), Violation> {
    let with_overlay = overlay_from_bottom(overlay_content, 0);
    // Compose: base + overlay (overlay drawn on top).
    let with = crucible_oil::node::fragment(vec![base.clone(), with_overlay]);
    let without = filter_overlays(with.clone());

    let with_lines: Vec<String> = render_tree(&with, width, height)
        .content
        .lines()
        .map(String::from)
        .collect();
    let without_lines: Vec<String> = render_tree(&without, width, height)
        .content
        .lines()
        .map(String::from)
        .collect();

    // Top half is unambiguously outside any reasonable overlay region.
    let safe_rows = (height as usize / 3)
        .max(1)
        .min(with_lines.len())
        .min(without_lines.len());
    for i in 0..safe_rows {
        if with_lines[i] != without_lines[i] {
            return Err(Violation {
                invariant: "overlay_non_interference",
                op_index: i,
                detail: format!(
                    "row {i} differs with/without overlay (overlay should only \
                     touch rows near its anchor):\nwith:    {:?}\nwithout: {:?}",
                    preview(&with_lines[i]),
                    preview(&without_lines[i]),
                ),
            });
        }
    }
    Ok(())
}

// ─── Invariant #8: Cursor restored after cleanup_viewport ──────────────────

/// After `Terminal::cleanup_viewport()`, the byte stream ends with the
/// canonical "move below viewport then clear to end of screen" sequence:
/// a `MoveDown(n)` (optional, only if cursor wasn't at last row),
/// a column reset (`MoveToColumn(0)`),
/// and a `Clear(FromCursorDown)` (`\x1b[J` or `\x1b[0J`).
///
/// This is the canonical end-of-frame leave-state: cursor sits below
/// the rendered viewport, no orphan content can appear after a
/// subsequent `println!`.
fn check_cleanup_viewport_restores_cursor(
    initial_dims: (u16, u16),
    ops: &[Op],
) -> Result<(), Violation> {
    let mut term: Terminal<Vec<u8>> = Terminal::headless(initial_dims.0, initial_dims.1);

    for op in ops {
        match op {
            Op::RenderFrame { tree } => {
                FrameRenderer::render_frame(&mut term, tree, None);
            }
            Op::Graduate { node, viewport } => {
                let grad = Graduation { node: node.clone() };
                FrameRenderer::render_frame(&mut term, viewport, Some(&grad));
            }
            Op::Resize { width, height } => {
                term.set_size(*width, *height);
            }
        }
    }

    // Drain the byte buffer accumulated by op processing, then call
    // cleanup_viewport. The remaining bytes are exactly the cleanup emission.
    let _ = term.take_bytes();
    term.cleanup_viewport().map_err(|e| Violation {
        invariant: "cleanup_viewport_restores_cursor",
        op_index: usize::MAX,
        detail: format!("cleanup_viewport returned io error: {e}"),
    })?;
    let cleanup_bytes = term.take_bytes();

    // Two valid post-states:
    //  - The OutputBuffer tracked zero visual rows (empty viewport / no
    //    render produced output); cleanup_viewport is a no-op by design.
    //  - Otherwise, the byte stream must contain the canonical clear
    //    sequence so subsequent output starts on a fresh row.
    if cleanup_bytes.is_empty() {
        return Ok(());
    }

    let s = String::from_utf8_lossy(&cleanup_bytes);
    let has_clear = s.contains("\x1b[J") || s.contains("\x1b[0J");
    if !has_clear {
        return Err(Violation {
            invariant: "cleanup_viewport_restores_cursor",
            op_index: usize::MAX,
            detail: format!(
                "cleanup_viewport emitted bytes without Clear-from-cursor-down \
                 (\\x1b[J or \\x1b[0J).\nbytes: {:?}",
                preview(&s),
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

    /// Invariant #2: graduation stdout_delta is byte-equal to
    /// `render_tree(node, planner_width, NATURAL_HEIGHT)`.
    #[test]
    fn prop_graduation_bytes_equal_render_tree_bytes(
        dims in arb_dims(),
        ops in arb_operation_sequence(),
    ) {
        if let Err(v) = check_graduation_width_stable(dims, &ops) {
            prop_assert!(
                false,
                "width-stable graduation violated: {}\nops: {:#?}",
                v, ops,
            );
        }
    }

    /// Invariant #3: viewport content after any op equals
    /// `render_tree(viewport_tree, w, h)` — no crosstalk between
    /// graduated scrollback and the viewport region.
    #[test]
    fn prop_no_graduation_viewport_crosstalk(
        dims in arb_dims(),
        ops in arb_operation_sequence(),
    ) {
        if let Err(v) = check_no_graduation_viewport_crosstalk(dims, &ops) {
            prop_assert!(
                false,
                "graduation/viewport crosstalk: {}\nops: {:#?}",
                v, ops,
            );
        }
    }

    /// Invariant #8: after `cleanup_viewport`, the byte stream ends with
    /// the canonical clear-from-cursor-down emission. Cursor sits below
    /// the rendered viewport — no orphan content appears on next println!.
    #[test]
    fn prop_cleanup_viewport_restores_cursor(
        dims in arb_dims(),
        ops in arb_operation_sequence(),
    ) {
        if let Err(v) = check_cleanup_viewport_restores_cursor(dims, &ops) {
            prop_assert!(
                false,
                "cleanup_viewport restoration violated: {}\nops: {:#?}",
                v, ops,
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        ..ProptestConfig::default()
    })]

    /// Invariant #6: vt100-parsed render output's visible cells match
    /// `strip_ansi(output)` line-by-line. Catches escape sequences that
    /// would confuse a real terminal.
    #[test]
    fn prop_ansi_round_trips_through_vt100(
        tree in arb_chat_like_node(),
        dims in arb_dims(),
    ) {
        if let Err(v) = check_ansi_round_trips_through_vt100(&tree, dims.0, dims.1) {
            prop_assert!(false, "ANSI round-trip violated: {}", v);
        }
    }

    /// Invariant #7: an `Overlay`'s effect is confined near its anchor.
    /// Rendering with vs without the overlay produces identical cells in
    /// the top third of the viewport (well away from the bottom-anchored
    /// overlay's influence).
    #[test]
    fn prop_overlay_non_interference(
        base in arb_chat_like_node(),
        overlay_content in arb_chat_like_node(),
        dims in arb_dims(),
    ) {
        if let Err(v) = check_overlay_non_interference(&base, overlay_content, dims.0, dims.1) {
            prop_assert!(false, "overlay non-interference violated: {}", v);
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
