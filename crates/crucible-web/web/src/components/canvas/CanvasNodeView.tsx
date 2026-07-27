import { Component, JSX, Match, Show, Switch, createMemo } from 'solid-js';
import { MarkdownPreview } from '../editor/MarkdownPreview';
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
          <div class="h-full overflow-auto px-3 py-2 text-sm" data-testid="canvas-text-node">
            <MarkdownPreview content={(props.node as { text: string }).text} />
          </div>
        </Match>

        <Match when={props.node.type === 'link'}>
          <LinkCard url={(props.node as { url: string }).url} />
        </Match>

        <Match when={props.node.type === 'file'}>
          <FileCard
            node={props.node as FileNode}
            rawUrlFor={props.rawUrlFor}
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

const LinkCard: Component<{ url: string }> = (props) => {
  const host = createMemo(() => {
    try {
      return new URL(props.url).host;
    } catch {
      return props.url;
    }
  });

  // A link card rather than an iframe by default: embedding silently contacts
  // a third party the moment a canvas is opened, which is a privacy decision
  // the user has not made just by drawing a box.
  return (
    <a
      class="flex h-full flex-col justify-center gap-1 px-3 no-underline"
      href={props.url}
      target="_blank"
      rel="noopener noreferrer"
      data-testid="canvas-link-node"
    >
      <span class="flex items-center gap-1.5 text-xs text-muted">
        <ExternalLink class="h-3 w-3" />
        {host()}
      </span>
      <span class="truncate text-sm text-shell-ink">{props.url}</span>
    </a>
  );
};

const FileCard: Component<{
  node: FileNode;
  rawUrlFor: (relPath: string) => string;
  onOpenFile?: (relPath: string, subpath?: string) => void;
}> = (props) => {
  const path = () => props.node.file;
  const name = () => path().split('/').pop() ?? path();

  return (
    <Switch fallback={<FileHeader node={props.node} onOpen={props.onOpenFile} />}>
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
        class="flex shrink-0 items-center gap-1.5 border-b border-subtle px-3 py-1.5 text-left text-xs font-medium text-shell-ink hover:bg-hover-wash"
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
