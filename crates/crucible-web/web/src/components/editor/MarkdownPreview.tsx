/**
 * Rendered markdown view of a note buffer — the reading half of the editor's
 * Edit ↔ Preview toggle. Renders through the same pipeline as chat messages
 * (wikilinks become `data-note` anchors), so previewed wikilinks get the
 * app-wide hover cards and click-to-open for free.
 */
import { Component, createResource } from 'solid-js';
import { renderMarkdownDocAsync, PROSE_CLASS } from '@/lib/markdown';
import { openNoteInEditor, stripFrontmatter } from '@/lib/note-actions';
import { statusBarStore } from '@/stores/statusBarStore';

const dirOf = (path?: string): string | undefined =>
  path ? path.replace(/\/[^/]*$/, '') : undefined;

export const MarkdownPreview: Component<{
  content: string;
  /** Absolute file path — its directory resolves relative image srcs. */
  path?: string;
  maxWidth?: number;
}> = (props) => {
  const [html] = createResource(
    () => [props.content, props.path] as const,
    ([content, path]) => renderMarkdownDocAsync(stripFrontmatter(content), dirOf(path)),
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
        class={`${PROSE_CLASS} mx-auto`}
        // Readable line length setting; falls back to the classic prose column.
        style={{ 'max-width': props.maxWidth ? `${props.maxWidth}px` : '768px' }}
        // eslint-disable-next-line solid/no-innerhtml
        innerHTML={html() ?? ''}
      />
    </div>
  );
};
