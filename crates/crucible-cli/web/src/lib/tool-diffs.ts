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
