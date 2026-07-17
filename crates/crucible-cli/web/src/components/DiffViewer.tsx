import { Component, For, Show, createSignal, createMemo } from 'solid-js';
import { highlighter, SHIKI_THEME, SHIKI_LANGS } from '@/lib/shiki';
import { languageFromFileName } from '@/lib/language-detection';
import { analyzeDiff, type DiffAnalysis, type DiffLine } from '@/lib/diff-stats';
import type { BundledLanguage, ThemedToken } from 'shiki';

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

const lineStyles = {
  add: 'bg-ok/15 text-ok',
  remove: 'bg-error/15 text-error',
  context: 'bg-surface-base text-muted',
};

const gutterStyles = {
  add: 'text-ok/60',
  remove: 'text-error/60',
  context: 'text-muted-dark',
};

const prefixChar = {
  add: '+',
  remove: '-',
  context: ' ',
};

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

  const maxLineNum = createMemo(() => {
    const lines = analysis().lines;
    let max = 0;
    for (const l of lines) {
      if (l.oldLineNum && l.oldLineNum > max) max = l.oldLineNum;
      if (l.newLineNum && l.newLineNum > max) max = l.newLineNum;
    }
    return max;
  });

  const gutterWidth = createMemo(() => Math.max(3, String(maxLineNum()).length));

  const effectiveLanguage = createMemo(() => {
    return props.language ?? languageFromFileName(props.fileName);
  });

  // Returns tokens for a single line of code, or null if highlighting unavailable
  // (highlighter not yet loaded, or language not in our eager set).
  const tokensForLine = (line: string): ThemedToken[] | null => {
    const lang = effectiveLanguage();
    if (lang === 'text' || lang === 'plaintext') return null;
    if (!(SHIKI_LANGS as readonly string[]).includes(lang)) return null;
    // Reactive read: when initializeHighlighter() resolves and flips the
    // signal, any Solid scope that called tokensForLine re-runs and the diff
    // upgrades from plain text to highlighted output.
    const h = highlighter();
    if (!h) return null;
    try {
      // codeToTokens returns one row per source line; we pass a single line in.
      // `lang` was just verified against SHIKI_LANGS, so the cast is safe.
      const result = h.codeToTokens(line, { lang: lang as BundledLanguage, theme: SHIKI_THEME });
      return result.tokens[0] ?? [];
    } catch {
      return null;
    }
  };

  const renderLine = (line: DiffLine) => {
    const gw = gutterWidth();
    // Memo wraps the tokensForLine call so its `highlighter()` read is
    // tracked. When initializeHighlighter() resolves and flips the signal,
    // this memo re-runs and Solid swaps the <Show> branch from plain text
    // to highlighted spans without us touching the surrounding For/render.
    const tokens = createMemo(() => tokensForLine(line.content));
    return (
      <div class={`flex ${lineStyles[line.type]} leading-5`}>
        <span
          class={`shrink-0 select-none text-right pr-1 border-r border-hairline ${gutterStyles[line.type]}`}
          style={{ width: `${gw + 1}ch` }}
        >
          {line.oldLineNum ?? ''}
        </span>
        <span
          class={`shrink-0 select-none text-right pr-1 border-r border-hairline ${gutterStyles[line.type]}`}
          style={{ width: `${gw + 1}ch` }}
        >
          {line.newLineNum ?? ''}
        </span>
        <span class={`shrink-0 select-none w-4 text-center ${gutterStyles[line.type]}`}>
          {prefixChar[line.type]}
        </span>
        <span class="flex-1 whitespace-pre overflow-x-auto">
          <Show when={tokens()} fallback={line.content || ' '}>
            {(toks) => (
              <For each={toks()}>
                {(tok) => <span style={{ color: tok.color }}>{tok.content}</span>}
              </For>
            )}
          </Show>
        </span>
      </div>
    );
  };

  return (
    <div class={props.hideHeader ? '' : 'rounded-lg border border-hairline overflow-hidden'}>
      {/* Header */}
      <Show when={!props.hideHeader}>
        <div class="flex items-center gap-3 px-3 py-2 bg-surface-elevated border-b border-hairline text-xs">
          <Show when={props.fileName}>
            <span class="font-mono text-shell-body truncate">{props.fileName}</span>
          </Show>
          <div class="flex items-center gap-2 ml-auto">
            <span class="text-ok font-mono">+{stats().additions}</span>
            <span class="text-error font-mono">-{stats().deletions}</span>
          </div>
        </div>
      </Show>

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
                class="w-full px-3 py-1 text-center text-xs text-muted-dark bg-surface-elevated hover:bg-hover-wash hover:text-shell-body transition-colors border-y border-hairline cursor-pointer"
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
