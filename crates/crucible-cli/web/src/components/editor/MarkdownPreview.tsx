/**
 * Rendered markdown view of a note buffer — the reading half of the editor's
 * Edit ↔ Preview toggle. Renders through the same pipeline as chat messages
 * (wikilinks become `data-note` anchors), so previewed wikilinks get the
 * app-wide hover cards and click-to-open for free.
 */
import { Component, createResource } from 'solid-js';
import { renderMarkdownAsync } from '@/lib/markdown';
import { openNoteInEditor, stripFrontmatter } from '@/lib/note-actions';
import { statusBarStore } from '@/stores/statusBarStore';

export const MarkdownPreview: Component<{ content: string }> = (props) => {
  const [html] = createResource(
    () => props.content,
    (content) => renderMarkdownAsync(stripFrontmatter(content)),
  );

  // The rendered HTML is not a component tree — delegate clicks the same way
  // chat messages do.
  const handleClick = (event: MouseEvent) => {
    const anchor = (event.target as Element | null)?.closest?.('[data-note]');
    if (!anchor) return;
    event.preventDefault();
    const note = anchor.getAttribute('data-note');
    if (note) void openNoteInEditor(note, statusBarStore.kilnPath() ?? undefined);
  };

  return (
    <div
      class="h-full overflow-y-auto bg-shell-panel px-6 py-4"
      data-testid="markdown-preview"
      onClick={handleClick}
    >
      <div
        class="prose prose-invert prose-sm mx-auto max-w-3xl
          prose-headings:text-shell-ink prose-p:leading-relaxed prose-a:text-primary
          prose-pre:bg-neutral-900 prose-pre:rounded-lg
          prose-code:before:content-none prose-code:after:content-none"
        // eslint-disable-next-line solid/no-innerhtml
        innerHTML={html() ?? ''}
      />
    </div>
  );
};
