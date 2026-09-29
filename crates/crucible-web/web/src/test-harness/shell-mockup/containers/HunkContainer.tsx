/** One inline hunk on the mock store. It shows only while the hunk waits for a decision. */
import { Show, type Component } from 'solid-js';
import { HunkBlock } from '../components/review/HunkBlock';
import type { HunkView } from '../components/review/types';
import { decide, hunkLines, state } from '../state';

/** A hunk view that reads the store on each access. */
export const hunkView = (id: string): HunkView => ({
  id,
  get state() {
    return state.hunks[id]!.state;
  },
  get external() {
    return !!state.hunks[id]!.external;
  },
  get del() {
    return hunkLines(id).del;
  },
  get add() {
    return hunkLines(id).add;
  },
});

export const HunkContainer: Component<{ id: string }> = (props) => {
  const h = () => state.hunks[props.id]!;
  const author = () => state.sessions[h().session]!;
  return (
    <Show when={h().state === 'pending'}>
      <HunkBlock hunk={hunkView(props.id)} author={author()} onDecide={(accept) => decide([props.id], accept)} />
    </Show>
  );
};
