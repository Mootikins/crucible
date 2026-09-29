/**
 * The transcript as blocks: consecutive quiet calls (done, with no edit)
 * fold into one group; every other item stands alone. The last assistant
 * text keeps its meta line visible.
 */
import type { ToolItem, TranscriptItem } from './types';

export type Block = { kind: 'item'; it: TranscriptItem; last: boolean } | { kind: 'group'; items: ToolItem[] };

const isQuiet = (it: TranscriptItem): it is ToolItem => it.t === 'tool' && !it.hunk && it.st === 'ok';

export function blocks(items: TranscriptItem[]): Block[] {
  const out: Block[] = [];
  let quiet: ToolItem[] = [];
  const lastText = items.map((i) => i.t).lastIndexOf('text');
  const flush = () => {
    if (quiet.length > 1) out.push({ kind: 'group', items: quiet });
    else if (quiet.length === 1) out.push({ kind: 'item', it: quiet[0]!, last: false });
    quiet = [];
  };
  items.forEach((it, i) => {
    if (isQuiet(it)) {
      quiet.push(it);
      return;
    }
    flush();
    out.push({ kind: 'item', it, last: i === lastText });
  });
  flush();
  return out;
}
