import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import { diffLines } from 'diff';

interface DiffLine {
  type: 'add' | 'remove' | 'context';
  content: string;
  oldLineNum: number | null;
  newLineNum: number | null;
}

interface CollapsedSection {
  kind: 'collapsed';
  lines: DiffLine[];
  startIndex: number;
}

interface VisibleSection {
  kind: 'visible';
  lines: DiffLine[];
}

type DiffSection = CollapsedSection | VisibleSection;

interface Props {
  oldContent: string;
  newContent: string;
  fileName?: string;
}

const CONTEXT_LINES = 3;

function computeDiffLines(oldContent: string, newContent: string): DiffLine[] {
  const changes = diffLines(oldContent, newContent);
  const result: DiffLine[] = [];
  let oldLine = 1;
  let newLine = 1;

  for (const change of changes) {
    const lines = change.value.replace(/\n$/, '').split('\n');
    // Handle empty string edge case (empty file)
    if (lines.length === 1 && lines[0] === '' && change.value === '') continue;

    for (const line of lines) {
      if (change.added) {
        result.push({ type: 'add', content: line, oldLineNum: null, newLineNum: newLine });
        newLine++;
      } else if (change.removed) {
        result.push({ type: 'remove', content: line, oldLineNum: oldLine, newLineNum: null });
        oldLine++;
      } else {
        result.push({ type: 'context', content: line, oldLineNum: oldLine, newLineNum: newLine });
        oldLine++;
        newLine++;
      }
    }
  }

  return result;
}

function buildSections(lines: DiffLine[]): DiffSection[] {
  if (lines.length === 0) return [];

  // Find which lines are "interesting" (changed or within CONTEXT_LINES of a change)
  const interesting = new Set<number>();
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].type !== 'context') {
      for (let j = Math.max(0, i - CONTEXT_LINES); j <= Math.min(lines.length - 1, i + CONTEXT_LINES); j++) {
        interesting.add(j);
      }
    }
  }

  // If everything is interesting (small file or all changes), show everything
  if (interesting.size >= lines.length) {
    return [{ kind: 'visible', lines }];
  }

  const sections: DiffSection[] = [];
  let currentVisible: DiffLine[] = [];
  let currentCollapsed: DiffLine[] = [];
  let collapsedStart = 0;

  for (let i = 0; i < lines.length; i++) {
    if (interesting.has(i)) {
      // Flush any collapsed section
      if (currentCollapsed.length > 0) {
        sections.push({ kind: 'collapsed', lines: currentCollapsed, startIndex: collapsedStart });
        currentCollapsed = [];
      }
      currentVisible.push(lines[i]);
    } else {
      // Flush any visible section
      if (currentVisible.length > 0) {
        sections.push({ kind: 'visible', lines: currentVisible });
        currentVisible = [];
      }
      if (currentCollapsed.length === 0) {
        collapsedStart = i;
      }
      currentCollapsed.push(lines[i]);
    }
  }

  // Flush remaining
  if (currentVisible.length > 0) {
    sections.push({ kind: 'visible', lines: currentVisible });
  }
  if (currentCollapsed.length > 0) {
    sections.push({ kind: 'collapsed', lines: currentCollapsed, startIndex: collapsedStart });
  }

  return sections;
}

const lineStyles = {
  add: 'bg-emerald-950/70 text-emerald-300',
  remove: 'bg-red-950/70 text-red-300',
  context: 'bg-zinc-900 text-zinc-400',
};

const gutterStyles = {
  add: 'text-emerald-600/60',
  remove: 'text-red-600/60',
  context: 'text-zinc-600',
};

const prefixChar = {
  add: '+',
  remove: '-',
  context: ' ',
};

export const DiffViewer: Component<Props> = (props) => {
  const diffLines_ = createMemo(() => computeDiffLines(props.oldContent, props.newContent));
  const initialSections = createMemo(() => buildSections(diffLines_()));

  // Track which collapsed sections have been expanded
  const [expandedSections, setExpandedSections] = createSignal<Set<number>>(new Set());

  const toggleSection = (startIndex: number) => {
    setExpandedSections((prev) => {
      const next = new Set(prev);
      if (next.has(startIndex)) {
        next.delete(startIndex);
      } else {
        next.add(startIndex);
      }
      return next;
    });
  };

  const stats = createMemo(() => {
    const lines = diffLines_();
    return {
      additions: lines.filter((l) => l.type === 'add').length,
      deletions: lines.filter((l) => l.type === 'remove').length,
    };
  });

  const maxLineNum = createMemo(() => {
    const lines = diffLines_();
    let max = 0;
    for (const l of lines) {
      if (l.oldLineNum && l.oldLineNum > max) max = l.oldLineNum;
      if (l.newLineNum && l.newLineNum > max) max = l.newLineNum;
    }
    return max;
  });

  const gutterWidth = createMemo(() => Math.max(3, String(maxLineNum()).length));

  const renderLine = (line: DiffLine) => {
    const gw = gutterWidth();
    return (
      <div class={`flex ${lineStyles[line.type]} leading-5`}>
        <span
          class={`shrink-0 select-none text-right pr-1 border-r border-zinc-700/50 ${gutterStyles[line.type]}`}
          style={{ width: `${gw + 1}ch` }}
        >
          {line.oldLineNum ?? ''}
        </span>
        <span
          class={`shrink-0 select-none text-right pr-1 border-r border-zinc-700/50 ${gutterStyles[line.type]}`}
          style={{ width: `${gw + 1}ch` }}
        >
          {line.newLineNum ?? ''}
        </span>
        <span class={`shrink-0 select-none w-4 text-center ${gutterStyles[line.type]}`}>
          {prefixChar[line.type]}
        </span>
        <span class="flex-1 whitespace-pre overflow-x-auto">{line.content || ' '}</span>
      </div>
    );
  };

  return (
    <div class="rounded-lg border border-zinc-700/80 overflow-hidden">
      {/* Header */}
      <div class="flex items-center gap-3 px-3 py-2 bg-zinc-800/80 border-b border-zinc-700/50 text-xs">
        <Show when={props.fileName}>
          <span class="font-mono text-zinc-300 truncate">{props.fileName}</span>
        </Show>
        <div class="flex items-center gap-2 ml-auto">
          <span class="text-emerald-400 font-mono">+{stats().additions}</span>
          <span class="text-red-400 font-mono">-{stats().deletions}</span>
        </div>
      </div>

      {/* Diff body */}
      <div class="font-mono text-xs overflow-x-auto max-h-80 overflow-y-auto">
        <For each={initialSections()}>
          {(section) => (
            <Show
              when={section.kind === 'collapsed' && !expandedSections().has((section as CollapsedSection).startIndex)}
              fallback={
                <For each={section.lines}>{(line) => renderLine(line)}</For>
              }
            >
              <button
                onClick={() => toggleSection((section as CollapsedSection).startIndex)}
                class="w-full px-3 py-1 text-center text-xs text-zinc-500 bg-zinc-800/50 hover:bg-zinc-800 hover:text-zinc-300 transition-colors border-y border-zinc-700/30 cursor-pointer"
              >
                ··· {section.lines.length} lines unchanged ···
              </button>
            </Show>
          )}
        </For>
      </div>
    </div>
  );
};
