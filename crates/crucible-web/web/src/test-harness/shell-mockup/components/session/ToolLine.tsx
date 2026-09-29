/** One tool call: a quiet line that opens to its details. */
import { Show, type Component } from 'solid-js';
import { basename } from '../path';
import { DiffStat } from './DiffStat';
import { ToolBody } from './ToolBody';
import { ToolDetails } from './ToolDetails';
import { ToolRow } from './ToolRow';
import { ToolStatus } from './ToolStatus';
import { toolWords } from './tools';
import type { ToolItem, ToolLineHandlers } from './types';

export const ToolLine: Component<{ it: ToolItem; tools: ToolLineHandlers }> = (props) => {
  const words = () => toolWords(props.it.name);
  const hunk = () => (props.it.hunk ? props.tools.hunkFor(props.it.hunk) : undefined);
  const open = () => props.tools.isOpen(props.it.id);
  // A call that waits or failed has not edited yet: it takes the present verb.
  const verb = () => (props.it.st === 'ask' || props.it.st === 'err' ? words().now ?? words().past : words().past);
  return (
    <div class="mk-tl" data-call={props.it.id}>
      <ToolRow icon={words().icon} label={verb()} open={open()} onToggle={() => props.tools.onToggle(props.it.id)}>
        <Show when={props.it.path} fallback={<span class="mk-q">{props.it.arg}</span>}>
          {(path) => (
            <button type="button" class="mk-f" title={`Open ${path()}`} onClick={() => props.tools.onOpenPath(path())}>
              {basename(path())}.md
            </button>
          )}
        </Show>
        <Show when={props.it.hunk && props.it.st === 'review' && hunk()?.state !== 'rejected'}>
          <DiffStat add={hunk()?.add.length ?? 0} del={hunk()?.del.length ?? 0} />
        </Show>
        <ToolStatus st={props.it.st} out={props.it.out} hunkState={hunk()?.state} />
      </ToolRow>
      <ToolBody open={open()}>
        <ToolDetails
          hunk={hunk()}
          out={props.it.out}
          onShow={() => props.tools.onOpenPath(hunk()!.path)}
          onDecide={(accept) => props.tools.onDecide(props.it.hunk!, accept)}
        />
      </ToolBody>
    </div>
  );
};
