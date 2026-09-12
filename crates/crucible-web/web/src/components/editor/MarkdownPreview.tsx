/**
 * Rendered markdown view of a note buffer — the reading half of the editor's
 * Edit ↔ Preview toggle. Renders through the same pipeline as chat messages
 * (wikilinks become `data-note` anchors), so previewed wikilinks get the
 * app-wide hover cards and click-to-open for free.
 */
import { Component, createEffect, createResource, onCleanup } from 'solid-js';
import { hydrateOfflineImages } from '@/lib/offline/images';
import { mountPluginBlocks } from '@/components/blocks/mount';
import { renderMarkdownDocAsync, proseClass } from '@/lib/markdown';
import { extractFrontmatterBlock, renderFrontmatterCardHtml } from '@/lib/frontmatter';
import { makeMarkdownClickHandler } from '@/lib/markdown-click';
import { wikilinkTargetMatches } from '@/lib/backlink-context';

const dirOf = (path?: string): string | undefined =>
  path ? path.replace(/\/[^/]*$/, '') : undefined;

export const MarkdownPreview: Component<{
  content: string;
  /** Absolute file path — its directory resolves relative image srcs. */
  path?: string;
  /**
   * Kiln to resolve `[[wikilinks]]` against, overriding the globally-active one.
   *
   * A canvas card renders content belonging to the canvas's OWN kiln, which is
   * not necessarily the kiln the status bar is pointing at — without this a
   * card's links silently resolve into a different vault.
   */
  kiln?: string;
  maxWidth?: number;
  /** Scroll to the first rendered wikilink pointing at this note key —
   * backlinks hover previews open at the referencing section. */
  scrollToNote?: string;
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
  // `||`, not `??`: a surface whose kiln has not loaded yet passes the empty
  // string, which is not a kiln — resolving against it silently produces
  // whatever the server does with a blank root.
  // No fallback to the active kiln. This preview renders a specific file, and
  // that file's kiln does not change because the user switched kilns in the
  // navigator. A caller that cannot say which kiln its content belongs to has
  // content whose links cannot be followed, and that should be visible.
  const kiln = () => props.kiln || undefined;
  const handleClick = makeMarkdownClickHandler();

  let scrollHost: HTMLDivElement | undefined;
  let proseHost: HTMLDivElement | undefined;

  // Put a live component into every ```plugin placeholder once the HTML lands.
  // Re-runs whenever the rendered HTML changes, and disposes the previous
  // islands first: `innerHTML` replaces the nodes those roots were mounted on,
  // so without the disposer their effects and pending fetches outlive the DOM
  // they were drawing into.
  let disposeBlocks: (() => void) | undefined;
  createEffect(() => {
    const rendered = html();
    disposeBlocks?.();
    disposeBlocks = undefined;
    if (rendered === undefined || !proseHost) return;
    disposeBlocks = mountPluginBlocks(proseHost);
  });
  onCleanup(() => disposeBlocks?.());

  // Point images at the copies this device keeps, once the HTML lands. The
  // markdown pipeline rewrites `<img src>` synchronously and cannot await a
  // blob, so the swap happens here — and a kept kiln's attachment is stored on
  // its first view, which is what "notes only" means by fetched when opened.
  createEffect(() => {
    const rendered = html();
    if (rendered === undefined || !proseHost) return;
    const host = proseHost;
    void hydrateOfflineImages(host, () => props.kiln ?? null).catch(() => undefined);
  });

  // After the async render lands, jump to the wikilink that points at the
  // requested note (rendered wikilinks carry data-note = raw target text).
  createEffect(() => {
    const target = props.scrollToNote;
    if (!target || html() === undefined || !scrollHost) return;
    const anchors = scrollHost.querySelectorAll<HTMLElement>('[data-note]');
    for (const a of anchors) {
      if (wikilinkTargetMatches(a.getAttribute('data-note') ?? '', [target])) {
        a.scrollIntoView({ block: 'center' });
        break;
      }
    }
  });

  return (
    <div
      ref={scrollHost}
      class="h-full overflow-y-auto bg-shell-panel px-6 py-4"
      data-testid="markdown-preview"
      // Declares which kiln this content belongs to for the document-level
      // hover controller, which cannot be passed props. Same value the click
      // handler uses, so hover and click can never target different kilns.
      data-kiln={kiln()}
      onClick={handleClick}
    >
      <div
        ref={proseHost}
        class={`${proseClass()} mx-auto`}
        // Readable line length setting; falls back to the classic prose column.
        style={{ 'max-width': props.maxWidth ? `${props.maxWidth}px` : '768px' }}
        // eslint-disable-next-line solid/no-innerhtml
        innerHTML={html() ?? ''}
      />
    </div>
  );
};
