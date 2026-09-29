/**
 * The view model of a hunk. The real app reads `ComposedHunk`
 * (`lib/review-types.ts`) through `reviewStore.hunksForOpenPath`: its state is
 * `unreviewed` (here `pending`), `accepted` or `rejected`, and its lines come
 * from `before_content` and `after_content`. `absent` is mockup-only: a hunk
 * that a pending permission has not written yet.
 */
export type HunkState = 'pending' | 'accepted' | 'rejected' | 'absent';

/** Who made an edit: the session's title and identity colour. */
export interface HunkAuthor {
  title: string;
  color: string;
}

export interface HunkView {
  id: string;
  state: HunkState;
  /** The user's own edit (`isExternal()`: no tool call claims it). It cannot be rejected. */
  external: boolean;
  del: string[];
  add: string[];
}

export interface ChangeFileView {
  /** The note path, without `.md`. */
  path: string;
  hunks: HunkView[];
}
