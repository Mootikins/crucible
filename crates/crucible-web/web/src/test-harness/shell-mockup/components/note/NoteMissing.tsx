/**
 * A note whose text the mockup does not carry: its name, a line that says
 * so, and the hunks that wait on it. The real app loads every note with
 * `useGetFileContent`, so it has no such view.
 */
import type { Component, JSX } from 'solid-js';
import { basename } from '../path';

export const NoteMissing: Component<{ path: string; children?: JSX.Element }> = (props) => (
  <article class="mk-note">
    <h1>{basename(props.path)}</h1>
    <p class="mk-quiet">The mockup carries the text of six notes from the docs kiln. The real app reads {props.path}.md from the daemon.</p>
    {props.children}
  </article>
);
