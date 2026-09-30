/**
 * The frontmatter of a note, folded to one quiet line ("3 properties") with
 * a caret. A click opens the table: description, status and tags, with no
 * frame. The real app reads the frontmatter with `lib/frontmatter.ts`. The
 * fold is display state, so the view keeps it.
 */
import { For, Show, createEffect, createSignal, type Component } from 'solid-js';
import { Caret } from '../primitives/Caret';
import { Pill } from '../primitives/Pill';
import type { NoteProps } from './parse';

/** The properties that the table shows; the fold line counts these. */
const SHOWN = ['description', 'status', 'tags'] as const;

export const NoteProperties: Component<{ props: NoteProps }> = (p) => {
  const [open, setOpen] = createSignal(false);
  const count = () => SHOWN.filter((k) => p.props[k]).length;
  let body: HTMLDivElement | undefined;
  // `inert` keeps the folded rows out of the tab order. It is set as an
  // attribute, because Solid sets `inert={…}` as a property.
  createEffect(() => body?.toggleAttribute('inert', !open()));
  return (
    <div class="mk-propsfold" classList={{ open: open() }}>
      <button type="button" class="mk-propstoggle" aria-expanded={open()} onClick={() => setOpen((o) => !o)}>
        <Caret open={open()} />
        {count()} {count() === 1 ? 'property' : 'properties'}
      </button>
      {/* The grid row grows from 0fr to 1fr, so the height moves without a measure. */}
      <div class="mk-propsbody" ref={body}>
        {/* The clip holds no padding: a padding would stay in view at 0fr. */}
        <div class="mk-propsclip">
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
        </div>
      </div>
    </div>
  );
};
