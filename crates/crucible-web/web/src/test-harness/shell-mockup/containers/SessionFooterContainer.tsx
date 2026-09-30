/**
 * The area under a transcript, on the mock store: the permission request,
 * the queue, and the composer (or a line that says a plugin runs the
 * session). A port reads `pendingInteraction`, the queued messages and the
 * session knobs from ChatContext and `lib/query/`.
 */
import { For, Show, type Component } from 'solid-js';
import { Composer } from '../components/composer/Composer';
import { basename } from '../components/path';
import { PermissionCard } from '../components/session/PermissionCard';
import { QueuedMessage } from '../components/session/QueuedMessage';
import { RecordLine } from '../components/session/RecordLine';
import { answerPermission, removeQueued, send, sendQueuedNow, setState, state } from '../state';

export const SessionFooterContainer: Component<{ sid: string }> = (props) => {
  const s = () => state.sessions[props.sid]!;
  return (
    <>
      <Show when={state.perms[props.sid]}>
        {(p) => <PermissionCard file={basename(p().path)} lines={p().lines} onAnswer={(choice) => answerPermission(props.sid, choice)} />}
      </Show>
      <For each={state.queue[props.sid] ?? []}>
        {(text, i) => (
          <QueuedMessage text={text} onSendNow={() => sendQueuedNow(props.sid, i())} onRemove={() => removeQueued(props.sid, i())} />
        )}
      </For>
      <Show when={!s().plugin} fallback={<RecordLine>Started by a plugin</RecordLine>}>
        <Composer
          draft={state.drafts[props.sid] ?? ''}
          onDraft={(text) => setState('drafts', props.sid, text)}
          onSend={() => send(props.sid)}
          onStop={() => setState('sessions', props.sid, 'status', 'idle')}
          running={s().status === 'run'}
          mode={s().mode}
          model={s().model}
          workspace={s().group === 'No project' ? 'Session folder' : s().group}
          kiln={s().roots.includes('docs') ? 'docs' : 'No kiln'}
        />
      </Show>
    </>
  );
};
