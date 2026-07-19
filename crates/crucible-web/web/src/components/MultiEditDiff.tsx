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
    <div class="rounded-lg border border-hairline overflow-hidden">
      <div class="flex items-center gap-3 px-3 py-2 bg-surface-elevated border-b border-hairline text-xs">
        <span class="font-mono text-shell-body truncate">{props.fileName}</span>
        <span class="text-muted-dark text-[10px] uppercase tracking-wider">
          {props.edits.length} edits
        </span>
        <div class="flex items-center gap-2 ml-auto">
          <span class="text-ok font-mono">+{stats().add}</span>
          <span class="text-error font-mono">-{stats().remove}</span>
        </div>
      </div>
      <div class="divide-y divide-hairline">
        <For each={props.edits}>
          {(edit, i) => (
            <div>
              <div class="px-3 py-1 bg-surface-base text-[10px] uppercase tracking-wider text-muted-dark font-mono">
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
