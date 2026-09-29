/** The path of a note under its root, with the note's own name in bold. */
import { For, type Component } from 'solid-js';

export const Breadcrumb: Component<{ root: string; path: string }> = (props) => {
  const parts = () => props.path.split('/');
  return (
    <span class="mk-path">
      {props.root}
      <For each={parts()}>
        {(p, i) => (
          <>
            <span class="mk-sep">/</span>
            {i() === parts().length - 1 ? <b>{p}</b> : p}
          </>
        )}
      </For>
    </span>
  );
};
