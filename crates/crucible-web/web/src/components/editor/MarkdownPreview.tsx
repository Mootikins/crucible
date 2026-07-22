/**
 * Rendered markdown view of a note buffer — the reading half of the editor's
 * Edit ↔ Preview toggle. Renders through the same pipeline as chat messages
 * (wikilinks become `data-note` anchors), so previewed wikilinks get the
 * app-wide hover cards and click-to-open for free.
 */
import { Component, createResource } from 'solid-js';
import { renderMarkdownDocAsync, PROSE_CLASS } from '@/lib/markdown';
import { extractFrontmatterBlock, renderFrontmatterCardHtml } from '@/lib/frontmatter';
import { makeMarkdownClickHandler } from '@/lib/markdown-click';
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
    async ([content, path]) => {
      // Frontmatter renders as the Properties card (YAML and TOML), never as
      // body text. Unparseable frontmatter is simply omitted, matching the
      // old strip behavior.
      const fm = extractFrontmatterBlock(content);
      const body = fm ? content.slice(fm.bodyStart) : content;
      const card = fm?.entries?.length ? renderFrontmatterCardHtml(fm.entries) : '';
      return card + (await renderMarkdownDocAsync(body, dirOf(path)));
    },
  );

  // The rendered HTML is not a component tree — delegate clicks through the
  // implementation shared with chat (lib/markdown-click.ts): wikilinks open
  // notes, copy buttons copy, external/relative links behave identically.
  const handleClick = makeMarkdownClickHandler(() => statusBarStore.kilnPath() ?? undefined);

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
