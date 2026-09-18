import type { ToolCallDisplay } from './types';

export type ToolDiff =
  | { kind: 'single'; fileName: string; oldContent: string; newContent: string }
  | { kind: 'multi'; fileName: string; edits: { oldContent: string; newContent: string }[] };

/**
 * Convert the daemon's `FileDiff` projection into the per-file diffs the card
 * renders. The daemon already decided what the diff IS — synth for the
 * built-in mutators, ACP forwarding otherwise — so this only reshapes it:
 * several edits to one file become one `multi` diff, each distinct file its
 * own entry. `old_content: null` is a whole-file write (empty old side),
 * matching the shape `DiffViewer` renders for "no previous content".
 *
 * `diffs` arrives `undefined` when the event carries none (or the transcript
 * entry predates the field).
 */
export function toolDiffsFromWire(diffs?: ToolCallDisplay['diffs']): ToolDiff[] {
  const raw = diffs ?? [];
  const out: ToolDiff[] = [];
  for (const d of raw) {
    if (typeof d?.path !== 'string' || typeof d?.new_content !== 'string') continue;
    const edit = {
      oldContent: typeof d.old_content === 'string' ? d.old_content : '',
      newContent: d.new_content,
    };
    const same = out.find((diff) => diff.fileName === d.path);
    if (!same) {
      out.push({ kind: 'single', fileName: d.path, ...edit });
    } else if (same.kind === 'single') {
      out[out.indexOf(same)] = {
        kind: 'multi',
        fileName: d.path,
        edits: [
          { oldContent: same.oldContent, newContent: same.newContent },
          edit,
        ],
      };
    } else {
      same.edits.push(edit);
    }
  }
  return out;
}

/**
 * Apply a tool diff onto a file's current content, yielding the PROPOSED full
 * content — for showing the change in the editor's inline diff against the
 * current file. Write replaces the whole file; Edit/MultiEdit replace the first
 * occurrence of each `oldContent` (matching the daemon's edit semantics). An
 * `oldContent` that isn't found (already applied, or a stale match) is skipped,
 * so a completed edit simply shows no change rather than corrupting content.
 */
export function applyToolDiff(original: string, diff: ToolDiff): string {
  if (diff.kind === 'single') {
    // Write (empty oldContent) overwrites; Edit replaces the first match.
    if (diff.oldContent === '') return diff.newContent;
    return replaceFirstLiteral(original, diff.oldContent, diff.newContent);
  }
  let out = original;
  for (const e of diff.edits) {
    out = e.oldContent === '' ? e.newContent : replaceFirstLiteral(out, e.oldContent, e.newContent);
  }
  return out;
}

/**
 * Literal first-occurrence splice. NOT `String.replace(needle, replacement)`:
 * that reads `$&`, `` $` ``, `$'`, `$1`, `$$` in the REPLACEMENT as substitution
 * patterns, so an edit inserting shell/regex/jQuery code (`echo "$&"`, `$1`)
 * would be silently mangled. Splicing by index is also a single scan.
 */
function replaceFirstLiteral(haystack: string, needle: string, replacement: string): string {
  const at = haystack.indexOf(needle);
  if (at === -1) return haystack;
  return haystack.slice(0, at) + replacement + haystack.slice(at + needle.length);
}
