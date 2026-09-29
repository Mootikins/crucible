/**
 * A note's rendered text: the properties, then the body. The body breaks at
 * each inline hunk, and the caller draws the hunk. The wikilink handlers sit
 * on the article, so a link inside a hunk works as well.
 */
import { For, Show, type Component, type JSX } from 'solid-js';
import { Markdown } from '../primitives/Markdown';
import type { WikilinkEvents } from '../primitives/wikilinks';
import { NoteProperties } from './NoteProperties';
import { segments, type NoteProps } from './parse';

export interface NoteArticleProps {
  props: NoteProps;
  body: string;
  links: WikilinkEvents;
  renderHunk: (id: string) => JSX.Element;
}

export const NoteArticle: Component<NoteArticleProps> = (p) => (
  <article class="mk-note" {...p.links}>
    <Show when={Object.keys(p.props).length}>
      <NoteProperties props={p.props} />
    </Show>
    <For each={segments(p.body)}>{(seg) => (seg.kind === 'md' ? <Markdown source={seg.text} /> : p.renderHunk(seg.id))}</For>
  </article>
);
