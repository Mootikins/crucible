/**
 * The diffset values of the web client, without any request.
 *
 * `DiffsetSource` is a closed set. The daemon has one exhaustive match on it.
 * The switches here and in `diff-api.ts` are the matches of the web client,
 * and `unreachable` fails the compile when a variant has no branch.
 *
 * This module imports no API client, so the tab actions can use it.
 */
import type { components } from './api-schema';

type Schemas = components['schemas'];

export type Diffset = Schemas['Diffset'];
export type DiffsetSource = Schemas['DiffsetSource'];
export type DiffFileEntry = Schemas['DiffFileEntry'];
export type DiffFileText = Schemas['DiffFileText'];
export type UnreadableRoot = Schemas['UnreadableRoot'];
export type DiffComment = Schemas['ReviewCommentRow'];
export type ListedComment = Schemas['ListedCommentRow'];
export type CommentSide = Schemas['CommentSideRow'];
export type NewDiffComment = Schemas['CommentBody'];

/** Fails the compile when a new source variant has no branch here. */
export function unreachable(source: never): never {
  throw new Error(`Unknown diffset source: ${JSON.stringify(source)}`);
}

/**
 * The key of one diffset in the client: the tab key and the cache key.
 *
 * The daemon hashes a branch source with BLAKE3, and the client cannot compute
 * that hash. The client therefore writes the fields out. A git ref contains no
 * ":" and no "..", so the key is not ambiguous. The other two keys are equal to
 * the daemon's id.
 */
export function diffsetKey(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `branch:${source.root}:${source.base}..${source.head ?? ''}`;
    case 'session_record':
      return `session-${source.session}`;
    case 'proposal':
      return `proposal-${source.id}`;
    default:
      return unreachable(source);
  }
}

/**
 * The file that a diff pane scrolls to and expands when it opens.
 *
 * The Changes panel knows the root and the relative path of a file. A tool
 * call knows only the path that the tool got, which is usually absolute.
 * Without a root, the path matches the root and the relative path joined, or
 * the relative path alone.
 */
export interface DiffFocus {
  path: string;
  root?: string;
}

/**
 * A focus target in the tab metadata. Each request gets a new `seq`, so a
 * second click on one file focuses it again.
 */
export interface DiffFocusRequest extends DiffFocus {
  seq: number;
}

/** A path without its last "/". */
function trimSlash(path: string): string {
  return path.length > 1 ? path.replace(/\/+$/, '') : path;
}

/** Whether a focus target names this file. */
export function focusMatches(file: Pick<DiffFileEntry, 'root' | 'path'>, focus: DiffFocus): boolean {
  if (focus.root !== undefined) return trimSlash(file.root) === trimSlash(focus.root) && file.path === focus.path;
  const root = trimSlash(file.root);
  const joined = root.endsWith('/') ? `${root}${file.path}` : `${root}/${file.path}`;
  return joined === focus.path || file.path === focus.path;
}

/** The last segment of a path. */
function basename(path: string): string {
  return path.replace(/\/+$/, '').split('/').pop() || path;
}

/** The title of the tab of one diffset. */
export function diffsetTitle(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `Diff: ${basename(source.root)}`;
    case 'session_record':
      return 'Diff: session';
    case 'proposal':
      return 'Diff: proposal';
    default:
      return unreachable(source);
  }
}

/**
 * The header text of one diffset, for example "Branch working tree → master".
 * An empty base is the default branch, which the reply of the daemon names.
 */
export function diffsetLabel(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `Branch ${source.head ?? 'working tree'} → ${source.base || 'default branch'}`;
    case 'session_record':
      return `Session ${source.session}`;
    case 'proposal':
      return `Proposal ${source.id}`;
    default:
      return unreachable(source);
  }
}

/**
 * The first and the last line of a stored range, both inclusive.
 *
 * The daemon stores `end` as one past the last line. The text forms show the
 * last line, so the end is inclusive only in text.
 */
function inclusiveLines(start: number, end: number): [number, number] {
  return [start, Math.max(start, end - 1)];
}

/** "626" for one line, "626-628" for a range. */
function span(first: number, last: number): string {
  return last > first ? `${first}-${last}` : `${first}`;
}

/**
 * The reference form of a range: "path:626" or "path:626-628". A mention puts
 * "@" before it. It mirrors `reference` in `crucible-core/src/diff.rs`.
 */
export function referenceForm(path: string, start: number, end: number): string {
  const [first, last] = inclusiveLines(start, end);
  return `${path}:${span(first, last)}`;
}

/** The lines of a text, as Rust `str::lines` gives them: no last empty line, no "\r". */
function textLines(text: string): string[] {
  if (text === '') return [];
  const lines = text.split('\n');
  if (lines[lines.length - 1] === '') lines.pop();
  return lines.map((line) => line.replace(/\r$/, ''));
}

/**
 * The quickfix form of one comment: "path:626: text" for one line, and
 * "path:626: [626-628] text" for a range.
 *
 * The Vim default `errorformat` (`%f:%l:%m`) needs a ":" after the line
 * number, so the location has only the start line. A range goes at the start
 * of the message. Each further line of the text is indented by two spaces.
 * It mirrors `quickfix_line` in `crucible-core/src/diff.rs`.
 */
export function quickfixLine(comment: Pick<DiffComment, 'path' | 'line_range' | 'body'>): string {
  const [first, last] = inclusiveLines(comment.line_range.start, comment.line_range.end);
  const [head = '', ...rest] = textLines(comment.body);
  const range = last > first ? `[${first}-${last}] ` : '';
  return [`${comment.path}:${first}: ${range}${head}`, ...rest.map((line) => `  ${line}`)].join('\n');
}

/** The quickfix list of the open comments: one entry for each, in order. */
export function quickfixList(comments: readonly DiffComment[]): string {
  return comments
    .filter((c) => !c.resolved)
    .map((c) => `${quickfixLine(c)}\n`)
    .join('');
}
