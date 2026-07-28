import { Component, Show, createEffect, createMemo, createSignal, on, onCleanup } from 'solid-js';
import { CodeMirrorEditor } from '../editor/CodeMirrorEditor';
import { MarkdownPreview } from '../editor/MarkdownPreview';
import { getFileContent, saveFileContent } from '@/lib/api';
import { openNoteInEditor } from '@/lib/note-actions';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { Code, Eye } from '@/lib/icons';

/**
 * The markdown surface of a canvas card.
 *
 * Text cards and note cards render through **exactly** this component, so they
 * are indistinguishable in styling: same markdown engine, same typography, no
 * chrome or filename header on either. In Obsidian a note card is a portal onto
 * the file rather than a differently-decorated widget, and the moment one kind
 * of card grows a header the two stop reading as the same thing.
 *
 * The only difference between them is where the text lives — inline in the
 * canvas for a text card, in a file for a note card — which is a persistence
 * concern, handled by the two thin wrappers below.
 */
const Markdown: Component<{
  content: string;
  path: string;
  /** The canvas's kiln — wikilinks must resolve here, not in the active one. */
  kiln?: string;
  editable: boolean;
  onChange: (next: string) => void;
  onCommit?: () => void;
}> = (props) => {
  // Vim keybindings come from the SAME global setting the main editors read —
  // a canvas card is an editor, and having it ignore the user's modal-editing
  // preference makes it the one place in the app where muscle memory fails.
  const { settings } = useSettingsSafe();

  // Per-card and per-session: reaching for raw text is a momentary thing, and
  // persisting it would mean a card silently opening as source days later.
  const [sourceMode, setSourceMode] = createSignal(false);

  // A canvas belongs to ONE kiln and its cards' links resolve there. The
  // marker rides the card body rather than the preview alone, because the
  // hover popovers are wired at the document level and fire over the editor's
  // wikilink decorations too — an edited card must not become the one place
  // where links escape into the active kiln.
  return (
  <Show
    when={props.editable}
    fallback={
      // Read-only until the card is focused. A canvas is mostly read at a
      // glance, and mounting an editor per visible card costs far more than it
      // buys — but the rendered output is the same engine either way, so
      // entering edit mode does not reflow the card.
      <div class="canvas-card-body h-full overflow-auto">
        <MarkdownPreview content={props.content} path={props.path} kiln={props.kiln} />
      </div>
    }
  >
    <div class="canvas-card-body relative h-full overflow-auto">
      {/* The live/source switch every other markdown surface has. A card had
          no way to reach raw text at all — live preview was hardcoded — so
          fixing a link's syntax or a fence meant opening the note elsewhere.
          Reading-vs-edit is deliberately NOT duplicated here: on a canvas that
          is the click/double-click gate, and a second control for it would be
          two ways to say the same thing. */}
      <button
        type="button"
        class="canvas-card-mode absolute right-1 top-1 z-10 flex h-[1.125rem] w-[1.125rem] items-center justify-center rounded-sm border border-hairline bg-surface-elevated/90 text-muted-dark hover:border-hairline-strong hover:text-shell-ink"
        title={sourceMode() ? 'Live preview' : 'Source'}
        aria-label={sourceMode() ? 'Live preview' : 'Source'}
        data-testid="canvas-card-mode-toggle"
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => setSourceMode((v) => !v)}
      >
        <Show when={sourceMode()} fallback={<Code class="h-3 w-3" />}>
          <Eye class="h-3 w-3" />
        </Show>
      </button>
      <CodeMirrorEditor
        content={props.content}
        path={props.path}
        onChange={props.onChange}
        onSave={() => props.onCommit?.()}
        vimMode={settings.editor.vimMode}
        // Without this, CodeMirror installs no wikilink decorations at all
        // (`CodeMirrorEditor` gates the whole bundle on it), so entering edit
        // mode silently stripped a card's links: no hover, no Ctrl+Click.
        onFollowLink={(target) => void openNoteInEditor(target, props.kiln)}
        livePreview={!sourceMode()}
      />
    </div>
    </Show>
  );
};

/** A text card: markdown stored inline in the canvas document. */
export const CanvasTextCard: Component<{
  text: string;
  nodeId: string;
  kiln?: string;
  editable: boolean;
  onChange: (next: string) => void;
}> = (props) => (
  <div class="h-full w-full" data-testid="canvas-text-node">
    <Markdown
      content={props.text}
      path={`${props.kiln ?? ''}/canvas-${props.nodeId}.md`}
      kiln={props.kiln}
      editable={props.editable}
      onChange={props.onChange}
    />
  </div>
);

/**
 * A note card: a live view of a real file.
 *
 * Edits write back to the source note. The canvas document stores only the
 * path, so note edits never touch the `.canvas` file and cannot collide with
 * canvas-level undo — two documents, two histories.
 */
export const CanvasNoteCard: Component<{
  absPath: string;
  kiln?: string;
  editable: boolean;
}> = (props) => {
  const [content, setContent] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [dirty, setDirty] = createSignal(false);

  // `props.absPath` is DERIVED from the canvas document (kiln + the node's file
  // field), so reading it inside an effect subscribes that effect to the whole
  // document — and the document is replaced on every drag frame. The effect
  // then re-ran continuously, blanking the content and refetching, which is
  // what the "Loading…" flicker was. A memo collapses that back to the string:
  // it only notifies when the path actually changes.
  const path = createMemo(() => props.absPath);

  createEffect(
    on(path, (current) => {
      // Flush before switching. Resetting `content` with a save still queued
      // meant the timer either saw `null` and silently dropped the edit, or
      // fired after the new file loaded and wrote ITS content back over the
      // old path.
      flushPending();
      setError(null);
      setContent(null);
      getFileContent(current)
        .then((text: string) => setContent(text))
        .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
    }),
  );

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  // The path the pending edit belongs to. A queued save must land on the file
  // it was typed into, not on whichever file the card is showing when the timer
  // fires.
  let pendingPath: string | null = null;
  onCleanup(() => {
    clearTimeout(saveTimer);
    // Virtualization unmounts cards that leave the viewport. Without this,
    // panning away from a card mid-edit would discard the edit silently —
    // virtualization would become data loss.
    if (dirty()) void flush();
  });

  /** Write the pending edit to the path it was made against. */
  const flush = async () => {
    const text = content();
    const target = pendingPath ?? props.absPath;
    if (text === null) return;
    try {
      await saveFileContent(target, text);
      if (target === props.absPath) setDirty(false);
      pendingPath = null;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  /** Synchronously hand off any queued edit before the card changes file. */
  const flushPending = () => {
    clearTimeout(saveTimer);
    if (dirty() && content() !== null) void flush();
    setDirty(false);
    pendingPath = null;
  };

  const onChange = (next: string) => {
    setContent(next);
    setDirty(true);
    pendingPath = props.absPath;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 800);
  };

  return (
    <div class="h-full w-full" data-testid="canvas-note-embed">
      <Show when={error()}>
        <div class="px-3 py-2 text-xs text-error" data-testid="canvas-embed-error">
          {error()}
        </div>
      </Show>
      <Show
        when={content() !== null}
        fallback={<div class="px-3 py-2 text-xs text-muted">Loading…</div>}
      >
        <Markdown
          content={content()!}
          path={props.absPath}
          kiln={props.kiln}
          editable={props.editable}
          onChange={onChange}
          onCommit={() => void flush()}
        />
      </Show>
    </div>
  );
};
