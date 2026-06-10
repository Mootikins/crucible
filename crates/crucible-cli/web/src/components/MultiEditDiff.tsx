import { Component, For, createMemo } from 'solid-js';
import { DiffViewer } from './DiffViewer';
import { languageFromFileName } from '@/lib/language-detection';
import { analyzeDiff } from '@/lib/diff-stats';

interface Edit {
  oldContent: string;
  newContent: string;
}

interface Props {
  fileName: string;
  edits: Edit[];
}

export const MultiEditDiff: Component<Props> = (props) => {
  const language = createMemo(() => languageFromFileName(props.fileName));

  // Compute analysis once per edit and reuse it for both the outer +/- header
  // and the inner DiffViewer (via precomputedAnalysis). Without this,
  // analyzeDiff() would run twice per edit and risk drift between paths.
  const analyses = createMemo(() => props.edits.map((e) => analyzeDiff(e.oldContent, e.newContent)));

  const stats = createMemo(() => {
    let add = 0;
    let remove = 0;
    for (const a of analyses()) {
      add += a.additions;
      remove += a.deletions;
    }
    return { add, remove };
  });

  return (
    <div class="rounded-lg border border-zinc-700/80 overflow-hidden">
      <div class="flex items-center gap-3 px-3 py-2 bg-zinc-800/80 border-b border-zinc-700/50 text-xs">
        <span class="font-mono text-zinc-300 truncate">{props.fileName}</span>
        <span class="text-zinc-500 text-[10px] uppercase tracking-wider">
          {props.edits.length} edits
        </span>
        <div class="flex items-center gap-2 ml-auto">
          <span class="text-emerald-400 font-mono">+{stats().add}</span>
          <span class="text-red-400 font-mono">-{stats().remove}</span>
        </div>
      </div>
      <div class="divide-y divide-zinc-700/30">
        <For each={props.edits}>
          {(edit, i) => (
            <div>
              <div class="px-3 py-1 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500 font-mono">
                Edit {i() + 1} of {props.edits.length}
              </div>
              <DiffViewer
                oldContent={edit.oldContent}
                newContent={edit.newContent}
                language={language()}
                hideHeader
                precomputedAnalysis={analyses()[i()]}
              />
            </div>
          )}
        </For>
      </div>
    </div>
  );
};
