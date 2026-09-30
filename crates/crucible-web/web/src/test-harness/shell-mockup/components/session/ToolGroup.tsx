/** Consecutive quiet calls, folded into one counted line: "Read a note, searched notes ×2". */
import { For, type Component } from 'solid-js';
import { Sparkles } from '@/lib/icons';
import { ToolBody } from './ToolBody';
import { ToolLine } from './ToolLine';
import { ToolRow } from './ToolRow';
import { TOOL } from './tools';
import type { ToolItem, ToolLineHandlers } from './types';

function summary(items: ToolItem[]): string {
  const counts = new Map<string, number>();
  for (const it of items) {
    const noun = TOOL[it.name]?.noun ?? it.name;
    counts.set(noun, (counts.get(noun) ?? 0) + 1);
  }
  const joined = [...counts].map(([noun, n]) => (n > 1 ? `${noun} ×${n}` : noun)).join(', ');
  return joined.charAt(0).toUpperCase() + joined.slice(1);
}

export const ToolGroup: Component<{ items: ToolItem[]; tools: ToolLineHandlers }> = (props) => {
  const id = () => `grp-${props.items[0]!.id}`;
  return (
    <div class="mk-tl">
      <ToolRow icon={Sparkles} label={summary(props.items)} open={props.tools.isOpen(id())} onToggle={() => props.tools.onToggle(id())} />
      <ToolBody open={props.tools.isOpen(id())}>
        <For each={props.items}>{(it) => <ToolLine it={it} tools={props.tools} />}</For>
      </ToolBody>
    </div>
  );
};
