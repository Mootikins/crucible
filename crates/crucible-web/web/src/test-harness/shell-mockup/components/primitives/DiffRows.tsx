/** Shared numbered rows for the permission preview and the wider review pane. */
import { For, createMemo, type Component } from 'solid-js';
import { diffWordsWithSpace } from 'diff';
import type { BundledLanguage } from 'shiki';
import type { DiffLine } from '@/lib/diff-stats';
import { highlighter, SHIKI_LANGS, SHIKI_THEMES } from '@/lib/shiki';
import { languageFromFileName } from '@/lib/language-detection';
import { tweaks } from '../../tweaks';

const MARK = { add: '+', remove: '−', context: '' } as const;
type Piece = { text: string; changed: boolean; color?: string };

export const DiffRows: Component<{
  rows: readonly DiffLine[];
  emphasis?: boolean;
  fileName?: string;
}> = (props) => {
  const pieces = createMemo(() => {
    const rows = props.rows;
    const output: Piece[][] = rows.map((row) => [{ text: row.content || ' ', changed: false }]);
    if (props.emphasis) {
      // Compare whole changed runs, so additions spanning several lines keep
      // their word alignment instead of pairing unrelated lines by position.
      for (let start = 0; start < rows.length;) {
        if (rows[start].type === 'context') {
          start++;
          continue;
        }
        let end = start;
        while (end < rows.length && rows[end].type !== 'context') end++;
        const indices = (side: 'add' | 'remove') =>
          Array.from({ length: end - start }, (_, i) => start + i).filter(
            (i) => rows[i].type === side,
          );
        const removed = indices('remove'),
          added = indices('add');
        const changes = diffWordsWithSpace(
          removed.map((i) => rows[i].content).join('\n'),
          added.map((i) => rows[i].content).join('\n'),
        );
        for (const side of ['remove', 'add'] as const) {
          const slots = side === 'remove' ? removed : added;
          slots.forEach((i) => (output[i] = []));
          let line = 0;
          for (const part of changes) {
            if (side === 'remove' ? part.added : part.removed) continue;
            const lines = part.value.split('\n');
            lines.forEach((text, i) => {
              if (i) line++;
              if (text && slots[line] !== undefined)
                output[slots[line]].push({ text, changed: !!(part.added || part.removed) });
            });
          }
        }
        start = end;
      }
    }
    const h = highlighter();
    const lang = props.fileName ? languageFromFileName(props.fileName) : 'text';
    if (
      !h ||
      lang === 'text' ||
      lang === 'plaintext' ||
      !(SHIKI_LANGS as readonly string[]).includes(lang)
    )
      return output;
    // Tokenize each complete side, retaining multiline syntax state. Split
    // tokens only where a word-change boundary needs a different background.
    for (const side of ['remove', 'add'] as const) {
      const indices = rows
        .map((_, i) => i)
        .filter((i) => rows[i].type !== (side === 'remove' ? 'add' : 'remove'));
      const tokens = h.codeToTokens(indices.map((i) => rows[i].content).join('\n'), {
        lang: lang as BundledLanguage,
        theme: SHIKI_THEMES[tweaks.theme],
      }).tokens;
      indices.forEach((index, line) => {
        if (rows[index].type === 'context' && side === 'remove') return;
        const original = output[index];
        const styled: Piece[] = [];
        let part = 0,
          used = 0;
        for (const token of tokens[line] ?? []) {
          let offset = 0;
          while (offset < token.content.length && part < original.length) {
            const count = Math.min(
              token.content.length - offset,
              original[part].text.length - used,
            );
            styled.push({
              text: token.content.slice(offset, offset + count),
              changed: original[part].changed,
              color: token.color,
            });
            offset += count;
            used += count;
            if (used === original[part].text.length) {
              part++;
              used = 0;
            }
          }
        }
        if (styled.length) output[index] = styled;
      });
    }
    return output;
  });
  return (
    <div class="mk-diff-rows">
      <For each={props.rows}>
        {(row, i) => (
          <div class={`mk-diff-row ${row.type}`}>
            <span class="n">{row.oldLineNum ?? ''}</span>
            <span class="n">{row.newLineNum ?? ''}</span>
            <span class="m">{MARK[row.type]}</span>
            <span class="c">
              <For each={pieces()[i()]}>
                {(piece) => (
                  <span
                    class={piece.changed ? 'mk-diff-word' : undefined}
                    style={piece.color ? { color: piece.color } : undefined}
                  >
                    {piece.text}
                  </span>
                )}
              </For>
            </span>
          </div>
        )}
      </For>
    </div>
  );
};
