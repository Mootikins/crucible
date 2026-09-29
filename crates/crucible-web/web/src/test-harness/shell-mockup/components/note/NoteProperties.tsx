/** The frontmatter of a note as a small table: description, status and tags. */
import { For, Show, type Component } from 'solid-js';
import { Pill } from '../primitives/Pill';
import type { NoteProps } from './parse';

export const NoteProperties: Component<{ props: NoteProps }> = (p) => (
  <dl class="mk-props">
    <Show when={p.props.description}>
      <dt>description</dt>
      <dd>{p.props.description as string}</dd>
    </Show>
    <Show when={p.props.status}>
      <dt>status</dt>
      <dd class="mk-status">{p.props.status as string}</dd>
    </Show>
    <Show when={p.props.tags}>
      <dt>tags</dt>
      <dd>
        <For each={p.props.tags as string[]}>{(t) => <Pill kind="tag">#{t}</Pill>}</For>
      </dd>
    </Show>
  </dl>
);
