/** The review tab of one session, on the mock store. A port reads the review store of the session. */
import { Show, createMemo, createSignal, type Component } from 'solid-js';
import { openNote } from '../actions';
import { ChangesView } from '../components/review/ChangesView';
import { ChangesControls, type ReviewScope } from '../components/review/ChangesControls';
import { ChunkExtras } from '../components/review/ChunkExtras';
import { tweaks } from '../tweaks';
import type { ChangeFileView } from '../components/review/types';
import { decide, pendingHunks, setState, state } from '../state';
import { hunkView } from './HunkContainer';

export const ChangesContainer: Component<{ sid: string }> = (props) => {
  // The variant "B with A's controls": the scope and the filter. The mock has
  // no turns, so "Last turn" shows the same hunks as "Session".
  const withControls = () => tweaks.changesControls === 'ab';
  const [scope, setScope] = createSignal<ReviewScope>('session');
  const [unreviewedOnly, setUnreviewedOnly] = createSignal(false);
  const ids = () =>
    Object.entries(state.hunks)
      .filter(([, h]) => h.session === props.sid && h.state !== 'absent')
      .filter(([, h]) => !(withControls() && unreviewedOnly()) || h.state === 'pending')
      .map(([id]) => id);
  const files = createMemo((): ChangeFileView[] => {
    const out: Record<string, string[]> = {};
    for (const id of ids()) (out[state.hunks[id]!.path] ??= []).push(id);
    return Object.entries(out).map(([path, hs]) => ({ path, hunks: hs.map(hunkView) }));
  });
  const owed = () => pendingHunks(props.sid);
  return (
    <ChangesView
      session={state.sessions[props.sid]!}
      files={files()}
      pending={owed().length}
      onDecideAll={(accept) => decide(owed(), accept)}
      onDecide={(id, accept) => decide([id], accept)}
      onOpenFile={(path) => openNote(path)}
      controls={
        <Show when={withControls()}>
          <ChangesControls scope={scope()} onScope={setScope} unreviewedOnly={unreviewedOnly()} onUnreviewedOnly={setUnreviewedOnly} />
        </Show>
      }
      hunkExtras={(h) => (
        <Show when={withControls()}>
          <ChunkExtras
            decided={h.state === 'accepted' || h.state === 'rejected'}
            onUndo={() => setState('hunks', h.id, 'state', 'pending')}
            onComment={() => undefined}
          />
        </Show>
      )}
    />
  );
};
