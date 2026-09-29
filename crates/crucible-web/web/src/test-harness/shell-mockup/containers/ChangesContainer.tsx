/** The review tab of one session, on the mock store. A port reads the review store of the session. */
import { createMemo, type Component } from 'solid-js';
import { openNote } from '../actions';
import { ChangesView } from '../components/review/ChangesView';
import type { ChangeFileView } from '../components/review/types';
import { decide, pendingHunks, state } from '../state';
import { hunkView } from './HunkContainer';

export const ChangesContainer: Component<{ sid: string }> = (props) => {
  const ids = () => Object.entries(state.hunks).filter(([, h]) => h.session === props.sid && h.state !== 'absent').map(([id]) => id);
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
    />
  );
};
