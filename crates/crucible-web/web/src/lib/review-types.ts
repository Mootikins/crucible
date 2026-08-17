/**
 * Wire types for the attributed-diff review surface.
 *
 * These mirror `crucible_core::session::{ComposedHunk, Comment, ReviewState}`
 * and `crucible_core::types::mode::ReviewPolicy` field-for-field, in the
 * daemon's snake_case serialization. They live here rather than in
 * `lib/types.ts` so the review surface can ship as one self-contained slice.
 *
 * One deliberate absence: there is no `external` field. Rust exposes it as
 * `ComposedHunk::is_external()`, a method, so it never crosses the wire —
 * `tool_call_ids.length === 0` IS the definition. `isExternal()` below is the
 * single place that knowledge lives on this side.
 */
import type { ModeDescriptor } from './types';

/** Per-composed-hunk review state. Matches the Rust enum's snake_case wire form. */
export type ReviewState = 'unreviewed' | 'accepted' | 'rejected';

/** 1-based, half-open — `start` is the first line, `end` is one PAST the last. */
interface LineRange {
  start: number;
  end: number;
}

export type CommentAuthor = 'human' | 'agent';

/**
 * A hunk of the composed diff (`session_base` → current worktree).
 *
 * The action unit of review. `before_content`/`after_content` are whole
 * newline-terminated blocks (empty for a pure insertion / deletion), which is
 * exactly the `oldContent`/`newContent` pair `DiffViewer` takes — the composed
 * diff is rendered by the existing viewer, not a second one.
 */
export interface ComposedHunk {
  /** Content-derived, stable across line shifts. The only handle for mutations. */
  id: string;
  /** Repository top level the hunk belongs to. */
  root: string;
  /** Path relative to `root`. */
  path: string;
  /** Range in `session_base` coordinates. */
  base_range: LineRange;
  /** Range in current-worktree coordinates — where it lives in the open buffer. */
  current_range: LineRange;
  before_content: string;
  after_content: string;
  /** Tool calls that contributed, in ledger order. Many-to-many, informational. */
  tool_call_ids: string[];
  state: ReviewState;
  /**
   * The agent re-applied a change the user rejected.
   *
   * Derived by the daemon, never client-settable. Such a hunk always reports
   * `state: 'unreviewed'`, so this adds no decision — only the history that
   * makes the grind visible instead of showing a fresh-looking hunk each round.
   */
  reapplied: boolean;
}

export interface ReviewComment {
  id: string;
  root: string;
  path: string;
  base_tree: string;
  line_range: LineRange;
  body: string;
  author: CommentAuthor;
  resolved: boolean;
  created_at: string;
}

/**
 * Effective review policy for the session's current mode.
 *
 * The daemon degrades this per agent capability before sending it
 * (`ModeDescriptor::degraded_for`), so what arrives here is what will actually
 * happen — an ACP session in `normal` reports `post_turn`, not `pre_write`.
 * Rendering the configured policy instead would be a lie about a safety
 * property, so nothing on this side re-derives it from the mode id.
 */
export type ReviewPolicy = 'none' | 'post_turn' | 'pre_write';

/**
 * `ModeDescriptor` as daemons carrying the review feature send it.
 *
 * Optional because `review_policy` is `#[serde(default)]` on the Rust side and
 * an older daemon simply omits it. Absent means "this daemon has no opinion" —
 * the UI shows no policy chip rather than guessing one.
 */
export interface ReviewAwareMode extends ModeDescriptor {
  review_policy?: ReviewPolicy;
}

/** Human-readable label for a policy, for the chip beside the mode selector. */
export const REVIEW_POLICY_LABELS: Record<ReviewPolicy, string> = {
  none: 'ungated',
  post_turn: 'review at turn end',
  pre_write: 'gated',
};

/**
 * A hunk nobody's ledger claims: the user's own editor, an async formatter, a
 * plugin writing directly. Shown for context so the composed diff stays
 * honest, never rejectable — reverting one destroys the user's concurrent work
 * while reporting that an agent edit was undone.
 */
export function isExternal(hunk: ComposedHunk): boolean {
  return hunk.tool_call_ids.length === 0;
}

/** Absolute path of a hunk: its root joined with its root-relative path. */
export function hunkPath(hunk: ComposedHunk): string {
  return `${hunk.root.replace(/\/$/, '')}/${hunk.path}`;
}
