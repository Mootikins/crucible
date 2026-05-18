import { Component, For, createMemo } from 'solid-js';
import { diffLines } from 'diff';
import { DiffViewer } from './DiffViewer';
import { languageFromFileName } from '@/lib/language-detection';

interface Edit {
  oldContent: string;
  newContent: string;
}

interface Props {
  fileName: string;
  edits: Edit[];
}

function countChanges(oldContent: string, newContent: string): { add: number; remove: number } {
  let add = 0;
  let remove = 0;
  for (const change of diffLines(oldContent, newContent)) {
    const lines = change.value.replace(/\n$/, '').split('\n');
    if (lines.length === 1 && lines[0] === '' && change.value === '') continue;
    if (change.added) add += lines.length;
    else if (change.removed) remove += lines.length;
  }
  return { add, remove };
}

export const MultiEditDiff: Component<Props> = (props) => {
  const language = createMemo(() => languageFromFileName(props.fileName));

  const stats = createMemo(() => {
    let add = 0;
    let remove = 0;
    for (const e of props.edits) {
      const c = countChanges(e.oldContent, e.newContent);
      add += c.add;
      remove += c.remove;
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
              />
            </div>
          )}
        </For>
      </div>
    </div>
  );
};
