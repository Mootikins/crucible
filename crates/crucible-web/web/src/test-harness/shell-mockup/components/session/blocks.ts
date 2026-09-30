/**
 * The transcript as blocks: consecutive quiet calls (done, with no edit)
 * fold into one group; every other item stands alone. The last assistant
 * text keeps its meta line visible. Only the final text of a completed turn
 * gets actions; copying includes all assistant text since the user message.
 */
import type { ToolItem, TranscriptItem } from './types';

export type Block =
  | { kind: 'item'; it: TranscriptItem; last: boolean; copyText?: string }
  | { kind: 'group'; items: ToolItem[] };

const isQuiet = (it: TranscriptItem): it is ToolItem =>
  it.t === 'tool' && !it.hunk && it.st === 'ok';

export function blocks(items: TranscriptItem[]): Block[] {
  const out: Block[] = [];
  let quiet: ToolItem[] = [];
  const lastText = items.map((i) => i.t).lastIndexOf('text');
  const flush = () => {
    if (quiet.length > 1) out.push({ kind: 'group', items: quiet });
    else if (quiet.length === 1) out.push({ kind: 'item', it: quiet[0]!, last: false });
    quiet = [];
  };
  let turnText: string[] = [];
  items.forEach((it, i) => {
    if (it.t === 'user') turnText = [];
    if (it.t === 'text') turnText.push(it.md);
    if (isQuiet(it)) {
      quiet.push(it);
      return;
    }
    flush();
    let next = items[i + 1];
    for (let n = i + 1; next?.t === 'record'; n++) next = items[n + 1];
    const complete =
      it.t === 'text' && (!next || next.t === 'user') && (!!it.elapsed || next?.t === 'user');
    out.push({
      kind: 'item',
      it,
      last: i === lastText,
      copyText: complete ? turnText.join('\n\n') : undefined,
    });
  });
  flush();
  return out;
}
