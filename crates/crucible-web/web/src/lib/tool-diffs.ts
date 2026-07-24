import type { ToolCallDisplay } from './types';

export type ToolDiff =
  | { kind: 'single'; fileName: string; oldContent: string; newContent: string }
  | { kind: 'multi'; fileName: string; edits: { oldContent: string; newContent: string }[] };

type ToolKind = 'edit' | 'write' | 'multiedit';

function classifyTool(name: string): ToolKind | null {
  const n = name.toLowerCase();
  if (n === 'multiedit') return 'multiedit';
  if (n === 'edit') return 'edit';
  if (n === 'write') return 'write';
  return null;
}

function parseArgs(args: string): Record<string, unknown> | null {
  if (!args) return null;
  try {
    const parsed = JSON.parse(args);
    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function asString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

export function extractDiffFromToolCall(call: ToolCallDisplay): ToolDiff | null {
  if (call.status === 'running') return null;

  const kind = classifyTool(call.name);
  if (!kind) return null;

  const args = parseArgs(call.args);
  if (!args) return null;

  const fileName = asString(args.file_path);
  if (!fileName) return null;

  if (kind === 'edit') {
    const oldContent = asString(args.old_string);
    const newContent = asString(args.new_string);
    if (oldContent === null || newContent === null) return null;
    return { kind: 'single', fileName, oldContent, newContent };
  }

  if (kind === 'write') {
    const newContent = asString(args.content);
    if (newContent === null) return null;
    return { kind: 'single', fileName, oldContent: '', newContent };
  }

  // multiedit
  const rawEdits = args.edits;
  if (!Array.isArray(rawEdits) || rawEdits.length === 0) return null;
  const edits: { oldContent: string; newContent: string }[] = [];
  for (const e of rawEdits) {
    if (typeof e !== 'object' || e === null) return null;
    const oldContent = asString((e as Record<string, unknown>).old_string);
    const newContent = asString((e as Record<string, unknown>).new_string);
    if (oldContent === null || newContent === null) return null;
    edits.push({ oldContent, newContent });
  }
  return { kind: 'multi', fileName, edits };
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
