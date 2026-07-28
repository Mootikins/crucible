import { Component, JSX, Match, Show, Switch, createMemo } from 'solid-js';
import { CanvasNoteCard, CanvasTextCard } from './CanvasCard';
import { ExternalLink, FileText, ShieldAlert } from '@/lib/icons';
import { resolveCanvasColor, type CanvasNode, type FileNode } from '@/lib/canvas-types';


/**
 * One canvas card.
 *
 * Positioning lives on the wrapper in the parent so this component only worries
 * about content — which is also what makes the level-of-detail swap cheap: at
 * low zoom the parent renders a placeholder and never mounts any of this.
 */

export interface CanvasNodeViewProps {
  node: CanvasNode;
  /** Absolute path resolver, so a note card can be a live view of the file. */
  absPathFor: (relPath: string) => string;
  /** Selected cards get a live editor; the rest render read-only. */
  editable?: boolean;
  /** The canvas's kiln, so card wikilinks resolve in the right vault. */
  kiln?: string;
  /** Text cards persist into the canvas document, so edits go back up. */
  onTextChange?: (nodeId: string, text: string) => void;
  /** Redacted by the server for failing containment; render an explanation. */
  rejectedReason?: string;
  /** Absolute path resolver for file nodes, so media can be fetched. */
  rawUrlFor: (relPath: string) => string;
  onOpenFile?: (relPath: string, subpath?: string) => void;
}

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i;
const PDF_EXT = /\.pdf$/i;
const AUDIO_EXT = /\.(mp3|wav|ogg|m4a|flac)$/i;
const VIDEO_EXT = /\.(mp4|webm|mov|mkv)$/i;
const CANVAS_EXT = /\.canvas$/i;

export const CanvasNodeView: Component<CanvasNodeViewProps> = (props) => {
  return (
    <Show
      when={!props.rejectedReason}
      fallback={<QuarantinedNode reason={props.rejectedReason ?? ''} />}
    >
      <Switch>
        <Match when={props.node.type === 'text'}>
          <CanvasTextCard
            text={(props.node as { text: string }).text}
            nodeId={props.node.id}
            kiln={props.kiln}
            editable={props.editable ?? false}
            onChange={(text) => props.onTextChange?.(props.node.id, text)}
          />
        </Match>

        <Match when={props.node.type === 'link'}>
          <LinkCard
            url={(props.node as { url: string }).url}
            interactive={props.editable ?? false}
          />
        </Match>

        <Match when={props.node.type === 'file'}>
          <FileCard
            node={props.node as FileNode}
            rawUrlFor={props.rawUrlFor}
            absPathFor={props.absPathFor}
            editable={props.editable}
            kiln={props.kiln}
            onOpenFile={props.onOpenFile}
          />
        </Match>

        {/* Groups are a labelled backdrop; the label is drawn by the parent so
            it stays legible above contained cards. */}
        <Match when={props.node.type === 'group'}>
          <div class="h-full w-full" />
        </Match>
      </Switch>
    </Show>
  );
};

/**
 * A reference the server refused and stripped.
 *
 * The offending path is deliberately not available to render — the server does
 * not send it back — so this explains the refusal without echoing the probe.
 */
const QuarantinedNode: Component<{ reason: string }> = (props) => (
  <div
    class="flex h-full flex-col items-center justify-center gap-1 px-3 text-center"
    data-testid="canvas-node-quarantined"
  >
    <ShieldAlert class="h-4 w-4 text-error" />
    <span class="text-xs font-medium text-error">Reference blocked</span>
    <span class="text-[11px] leading-tight text-muted-dark">{props.reason}</span>
  </div>
);

/**
 * Schemes a link node may navigate to.
 *
 * A `link` node's `url` is arbitrary text from the document, and containment
 * deliberately does not police it — a URL is not a filesystem reference. But it
 * lands in an `href`, so `javascript:` would execute on click and `data:` can
 * carry a document with the app's origin in its opener chain. Anything not on
 * this list renders as inert text.
 */
const SAFE_SCHEMES = ['http:', 'https:', 'mailto:'];

/** Schemes that may additionally be rendered *inside* the canvas. */
const EMBEDDABLE_SCHEMES = ['http:', 'https:'];

/**
 * Everything the frame is allowed to do.
 *
 * `allow-same-origin` is deliberately absent, which is the load-bearing part.
 * A link node's URL is arbitrary document text, so it can name this app's own
 * origin — and the web UI authenticates with a session cookie. With
 * `allow-same-origin` the frame would be same-origin with the app and could
 * read that cookie's pages and call the API as the user. Without it the frame
 * gets an opaque origin and cannot reach the app's storage, DOM or session,
 * whatever it points at. It also cannot pair with `allow-scripts` to remove
 * its own sandbox.
 */
const EMBED_SANDBOX = 'allow-scripts allow-popups allow-popups-to-escape-sandbox allow-forms';

/**
 * A web page on the canvas.
 *
 * Obsidian embeds the live page, so Crucible does too. That is a real change in
 * posture — opening a canvas now contacts every third party it references — and
 * the sandbox above is what makes it defensible rather than merely convenient.
 *
 * The frame is inert until the card is opened with a double-click, like every
 * other card type. Otherwise the iframe swallows the pointer and the card
 * cannot be dragged, selected, or marquee'd: the canvas would have holes in it.
 */
const LinkCard: Component<{ url: string; interactive?: boolean }> = (props) => {
  const parsed = createMemo(() => {
    try {
      return new URL(props.url);
    } catch {
      return null;
    }
  });
  const safe = createMemo(() => {
    const u = parsed();
    return u !== null && SAFE_SCHEMES.includes(u.protocol);
  });
  const embeddable = createMemo(() => {
    const u = parsed();
    return u !== null && EMBEDDABLE_SCHEMES.includes(u.protocol);
  });
  const host = createMemo(() => parsed()?.host ?? props.url);

  const chrome = (
    <a
      class="flex items-center gap-1.5 px-3 text-xs text-muted no-underline"
      classList={{
        // Embedded: a thin bar over the page, since the page itself is the
        // content. Otherwise the card IS the link, so it fills the box.
        'absolute inset-x-0 top-0 z-10 h-6 border-b border-hairline bg-surface-elevated/90 backdrop-blur-sm':
          embeddable(),
        'h-full flex-col justify-center gap-1': !embeddable(),
      }}
      href={safe() ? props.url : undefined}
      target="_blank"
      rel="noopener noreferrer"
      data-testid="canvas-link-node"
      data-unsafe-scheme={safe() ? undefined : 'true'}
      title={safe() ? props.url : 'Blocked: unsupported URL scheme'}
    >
      <span class="flex items-center gap-1.5 truncate text-xs text-muted">
        <ExternalLink class="h-3 w-3 shrink-0" />
        {host()}
      </span>
      <Show when={!embeddable()}>
        <span class="truncate text-sm text-shell-ink">{props.url}</span>
      </Show>
    </a>
  );

  return (
    <Show when={embeddable()} fallback={chrome}>
      <div class="relative h-full w-full overflow-hidden">
        {chrome}
        <iframe
          src={props.url}
          class="h-full w-full border-0 bg-white pt-6"
          classList={{ 'pointer-events-none': !props.interactive }}
          sandbox={EMBED_SANDBOX}
          referrerpolicy="no-referrer"
          loading="lazy"
          title={host()}
          data-testid="canvas-link-embed"
          data-interactive={props.interactive ? 'true' : 'false'}
        />
      </div>
    </Show>
  );
};

const MARKDOWN_EXT = /\.(md|markdown)$/i;

const FileCard: Component<{
  node: FileNode;
  rawUrlFor: (relPath: string) => string;
  absPathFor: (relPath: string) => string;
  editable?: boolean;
  kiln?: string;
  onOpenFile?: (relPath: string, subpath?: string) => void;
}> = (props) => {
  const path = () => props.node.file;
  const name = () => path().split('/').pop() ?? path();

  return (
    <Switch fallback={<FileHeader node={props.node} onOpen={props.onOpenFile} />}>
      <Match when={MARKDOWN_EXT.test(path())}>
        <CanvasNoteCard
          absPath={props.absPathFor(path())}
          kiln={props.kiln}
          editable={props.editable ?? false}
        />
      </Match>
      <Match when={IMAGE_EXT.test(path())}>
        <img
          class="h-full w-full object-cover"
          src={props.rawUrlFor(path())}
          alt={name()}
          loading="lazy"
          data-testid="canvas-media-node"
        />
      </Match>
      <Match when={VIDEO_EXT.test(path())}>
        <video class="h-full w-full object-cover" src={props.rawUrlFor(path())} controls />
      </Match>
      <Match when={AUDIO_EXT.test(path())}>
        <div class="flex h-full items-center px-3">
          <audio class="w-full" src={props.rawUrlFor(path())} controls />
        </div>
      </Match>
      <Match when={PDF_EXT.test(path())}>
        <object class="h-full w-full" data={props.rawUrlFor(path())} type="application/pdf">
          <FileHeader node={props.node} onOpen={props.onOpenFile} />
        </object>
      </Match>
      <Match when={CANVAS_EXT.test(path())}>
        <div class="flex h-full flex-col items-center justify-center gap-1">
          <FileText class="h-4 w-4 text-muted" />
          <span class="text-xs text-muted">{name()}</span>
          <button
            type="button"
            class="text-[11px] text-primary hover:underline"
            onClick={() => props.onOpenFile?.(path())}
          >
            Open canvas
          </button>
        </div>
      </Match>
    </Switch>
  );
};

/**
 * Header shown for a markdown file node in read-only mode. Phase 6 replaces the
 * body with a live editor; the header stays either way.
 */
const FileHeader: Component<{
  node: FileNode;
  onOpen?: (relPath: string, subpath?: string) => void;
  children?: JSX.Element;
}> = (props) => {
  const name = () => props.node.file.split('/').pop() ?? props.node.file;
  const accent = () => resolveCanvasColor(props.node.color);

  return (
    <div class="flex h-full flex-col" data-testid="canvas-file-node">
      <button
        type="button"
        class="flex shrink-0 items-center gap-1.5 px-3 py-2 text-left text-xs text-muted hover:text-shell-ink"
        style={accent() ? { color: accent() } : undefined}
        onClick={() => props.onOpen?.(props.node.file, props.node.subpath)}
      >
        <FileText class="h-3 w-3 shrink-0" />
        <span class="truncate">{name()}</span>
        <Show when={props.node.subpath}>
          <span class="shrink-0 text-muted-dark">{props.node.subpath}</span>
        </Show>
      </button>
      <div class="min-h-0 flex-1 overflow-hidden">{props.children}</div>
    </div>
  );
};
