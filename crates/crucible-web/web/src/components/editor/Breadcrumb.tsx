/** The path of a note under its root, with the note's own name in bold. */
import { For, type Component } from 'solid-js';
import { scrollFade } from '@/lib/scroll-fade';

export const Breadcrumb: Component<{ root: string; path: string }> = (props) => {
  const parts = () => props.path.split('/');
  return (
    <span class="note-breadcrumb" ref={scrollFade('x')}>
      {props.root}
      <For each={parts()}>
        {(p, i) => (
          <>
            <span class="note-breadcrumb-separator">/</span>
            {i() === parts().length - 1 ? <b>{p}</b> : p}
          </>
        )}
      </For>
    </span>
  );
};
