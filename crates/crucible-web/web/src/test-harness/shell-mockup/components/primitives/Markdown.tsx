/** Markdown, rendered to HTML by the app's own renderer. */
import type { Component } from 'solid-js';
import { renderMarkdown } from '@/lib/markdown';
import type { WikilinkEvents } from './wikilinks';

export const Markdown: Component<{ source: string; class?: string; links?: WikilinkEvents }> = (props) => (
  <div class={props.class} {...(props.links ?? {})} innerHTML={renderMarkdown(props.source)} />
);
