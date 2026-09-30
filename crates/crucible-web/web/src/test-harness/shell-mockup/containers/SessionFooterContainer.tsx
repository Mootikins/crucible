/**
 * The area under a transcript, on the mock store: the permission request,
 * the queue, and the composer (or a line that says a plugin runs the
 * session). A port reads `pendingInteraction`, the queued messages and the
 * session knobs from ChatContext and `lib/query/`.
 */
import { For, Show, type Component } from 'solid-js';
import { review, setReview } from '../review';
import { Button } from '../components/primitives/Button';
import { Composer } from '../components/composer/Composer';
import { basename } from '../components/path';
import { PermissionCard } from '../components/session/PermissionCard';
import { QueuedMessage } from '../components/session/QueuedMessage';
import { RecordLine } from '../components/session/RecordLine';
import {
  NO_PROJECT,
  answerPermission,
  removeQueued,
  send,
  sendQueuedNow,
  setState,
  state,
} from '../state';
import { permissionDiff } from './permissionDiff';

export const SessionFooterContainer: Component<{ sid: string }> = (props) => {
  const s = () => state.sessions[props.sid]!;
  return (
    <>
      <Show when={state.perms[props.sid]}>
        {(p) => (
          <PermissionCard
            file={basename(p().path)}
            diff={permissionDiff(
              p().path,
              state.notes[p().path.replace(/\.md$/, '')] ?? '',
              p().before,
              p().lines,
            )}
            onAnswer={(choice) => answerPermission(props.sid, choice)}
          />
        )}
      </Show>
      <For each={state.queue[props.sid] ?? []}>
        {(text, i) => (
          <QueuedMessage
            text={text}
            onSendNow={() => sendQueuedNow(props.sid, i())}
            onRemove={() => removeQueued(props.sid, i())}
          />
        )}
      </For>
      <Show when={!s().plugin} fallback={<RecordLine>Started by a plugin</RecordLine>}>
        <Show when={props.sid === 's1'}>
          <div class="mk-review-chips">
            <For each={review.comments.filter((c) => c.attached)}>
              {(c) => (
                <span class="mk-pill" title={c.text}>
                  {basename(c.path)} · comment{' '}
                  <Button
                    variant="ghost"
                    onClick={() =>
                      c.sent
                        ? setReview('comments', (x) => x.id === c.id, 'attached', false)
                        : setReview('comments', (rows) => rows.filter((x) => x.id !== c.id))
                    }
                  >
                    ×
                  </Button>
                </span>
              )}
            </For>
          </div>
        </Show>
        <Composer
          draft={state.drafts[props.sid] ?? ''}
          onDraft={(text) => setState('drafts', props.sid, text)}
          onSend={() => {
            const attached = props.sid === 's1' ? review.comments.filter((c) => c.attached) : [];
            if (attached.length)
              setState(
                'drafts',
                props.sid,
                `${state.drafts[props.sid] ?? ''}\n${attached.map((c) => `@comment:${c.id} ${c.text}`).join('\n')}`,
              );
            send(props.sid);
            attached.forEach((c) => {
              setReview('comments', (x) => x.id === c.id, 'sent', true);
              setReview('comments', (x) => x.id === c.id, 'attached', false);
            });
          }}
          onStop={() => setState('sessions', props.sid, 'status', 'idle')}
          running={s().status === 'run'}
          mode={s().mode}
          model={s().model}
          workspace={s().group === NO_PROJECT ? 'Session folder' : s().group}
          kiln={s().roots.includes('docs') ? 'docs' : 'No kiln'}
        />
      </Show>
    </>
  );
};
