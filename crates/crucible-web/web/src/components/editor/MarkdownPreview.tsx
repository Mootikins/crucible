/**
 * Rendered markdown view of a note buffer — the reading half of the editor's
 * Edit ↔ Preview toggle. Renders through the same pipeline as chat messages
 * (wikilinks become `data-note` anchors), so previewed wikilinks get the
 * app-wide hover cards and click-to-open for free.
 */
import { Component, createResource } from 'solid-js';
import { renderMarkdownDocAsync } from '@/lib/markdown';
import { openNoteInEditor, stripFrontmatter } from '@/lib/note-actions';
import { statusBarStore } from '@/stores/statusBarStore';

export const MarkdownPreview: Component<{ content: string; maxWidth?: number }> = (props) => {
  const [html] = createResource(
    () => props.content,
    (content) => renderMarkdownDocAsync(stripFrontmatter(content)),
  );

  // The rendered HTML is not a component tree — delegate clicks the same way
  // chat messages do: wikilink anchors open notes, code-block copy buttons
  // copy the adjacent <pre>.
  const handleClick = (event: MouseEvent) => {
    const target = event.target as Element | null;

    const copyBtn = target?.closest?.('[data-copy]');
    if (copyBtn) {
      event.preventDefault();
      const pre = copyBtn.closest('.md-codeblock')?.querySelector('pre');
      const code = pre?.textContent ?? '';
      if (code) {
        void navigator.clipboard?.writeText(code);
        const prev = copyBtn.textContent;
        copyBtn.textContent = 'Copied';
        copyBtn.classList.add('is-copied');
        setTimeout(() => {
          copyBtn.textContent = prev;
          copyBtn.classList.remove('is-copied');
        }, 1200);
      }
      return;
    }

    const anchor = target?.closest?.('[data-note]');
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
        class="prose prose-invert prose-sm mx-auto
          prose-headings:text-shell-ink prose-p:leading-relaxed prose-a:text-primary
          prose-pre:bg-surface-base prose-pre:rounded-lg
          prose-code:before:content-none prose-code:after:content-none"
        // Readable line length setting; falls back to the classic prose column.
        style={{ 'max-width': props.maxWidth ? `${props.maxWidth}px` : '768px' }}
        // eslint-disable-next-line solid/no-innerhtml
        innerHTML={html() ?? ''}
      />
    </div>
  );
};
