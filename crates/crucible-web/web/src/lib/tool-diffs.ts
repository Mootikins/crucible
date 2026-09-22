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
