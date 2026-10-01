import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import { DiffRows } from './DiffRows';
import { analyzeDiff, type DiffAnalysis, type DiffLine } from '@/lib/diff-stats';

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
  /** Override the language used for syntax highlighting. Defaults to inferring from fileName. */
  language?: string;
  /** When true, suppress the header bar (used by MultiEditDiff which provides its own). */
  hideHeader?: boolean;
  /**
   * Optional precomputed diff analysis. Lets MultiEditDiff compute analyzeDiff()
   * once per edit (for its stacked +/- header) and pass the result down so the
   * inner DiffViewer doesn't redo the work.
   */
  precomputedAnalysis?: DiffAnalysis;
}

const CONTEXT_LINES = 3;

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

export const DiffViewer: Component<Props> = (props) => {
  // When MultiEditDiff supplies precomputedAnalysis, reuse it; otherwise compute
  // locally. Either way the rest of the component sees one DiffAnalysis source.
  const analysis = createMemo<DiffAnalysis>(
    () => props.precomputedAnalysis ?? analyzeDiff(props.oldContent, props.newContent),
  );
  const initialSections = createMemo(() => buildSections(analysis().lines));

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
    const a = analysis();
    return { additions: a.additions, deletions: a.deletions };
  });

  return (
    <div class="diff-viewer">
      {/* Header */}
      <Show when={!props.hideHeader}>
        <div class="diff-viewer-header flex items-center gap-2">
          <Show when={props.fileName}>
            <span class="font-mono text-shell-body truncate">{props.fileName}</span>
          </Show>
          <div class="diff-viewer-stats flex items-center ml-auto">
            <span class="text-ok font-mono">+{stats().additions}</span>
            <span class="text-error font-mono">-{stats().deletions}</span>
          </div>
        </div>
      </Show>

      {/* Diff body */}
      <div class="diff-rows font-mono text-xs overflow-x-auto overflow-y-auto">
        <For each={initialSections()}>
          {(section) => (
            <Show
              when={section.kind === 'collapsed' && !expandedSections().has((section as CollapsedSection).startIndex)}
              fallback={
                <DiffRows rows={section.lines} emphasis fileName={props.fileName} language={props.language} />
              }
            >
              <button
                onClick={() => toggleSection((section as CollapsedSection).startIndex)}
                class="w-full px-3 py-1 text-center text-xs text-muted-dark bg-surface-elevated hover:bg-hover-wash hover:text-shell-body transition-colors cursor-pointer"
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
