/**
 * Backlinks panel — linked and unlinked mentions for the focused note.
 *
 * Linked mentions are notes whose wikilinks point at the focused note
 * (daemon graph edges). Unlinked mentions are plain-text references to
 * *other* notes inside the focused note, with one-click conversion to a
 * wikilink in the open editor buffer. Rows carry `data-note`, so the
 * app-wide hover preview works on them.
 */
import { Component, Show, For, createSignal, createResource, createMemo } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { getBacklinks, getConfig, getFileContent } from '@/lib/api';
import type { BacklinksResponse, UnlinkedMention } from '@/lib/types';
import { blockAtByteOffset, findLinkingBlock, type LinkingBlock } from '@/lib/backlink-context';
import { insertWikilink } from '@/lib/note-actions';
import { notificationActions } from '@/stores/notificationStore';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { RefreshCw } from '@/lib/icons';

/** Session kiln when set, else the daemon's configured default. */
function useKilnPath() {
  const { currentSession } = useSessionSafe();
  return createResource<string | null>(async () => {
    const sess = currentSession();
    if (sess?.kiln) return sess.kiln;
    try {
      const config = await getConfig();
      return config.kiln_path || null;
    } catch {
      return null;
    }
  });
}

/**
 * The server resolves notes by name or kiln-relative path. Editor paths are
 * absolute — strip the kiln prefix when possible, else use the file stem.
 */
export function noteKeyForPath(filePath: string, kiln: string | null): string {
  if (kiln) {
    const prefix = kiln.replace(/\/$/, '') + '/';
    if (filePath.startsWith(prefix)) {
      return filePath.slice(prefix.length);
    }
  }
  const base = filePath.split('/').pop() ?? filePath;
  return base.replace(/\.md$/, '');
}

const suggestionKey = (s: UnlinkedMention) => `${s.target}:${s.offset}:${s.mention}`;

export const BacklinksPanel: Component = () => {
  const editor = useEditorSafe();
  const [kilnPath] = useKilnPath();
  // Suggestions applied (or failed) since the last fetch — hidden locally.
  const [dismissed, setDismissed] = createSignal<Set<string>>(new Set());
  const [refreshTick, setRefreshTick] = createSignal(0);

  const focusedFile = createMemo(() => {
    const path = editor.activeFile();
    if (!path || !/\.(md|markdown)$/i.test(path)) return null;
    return path;
  });

  const [backlinks] = createResource(
    () => {
      const path = focusedFile();
      const kiln = kilnPath();
      if (!path || !kiln) return null;
      return { path, kiln, tick: refreshTick() };
    },
    async ({ path, kiln }): Promise<BacklinksResponse | null> => {
      setDismissed(new Set<string>());
      try {
        return await getBacklinks(kiln, noteKeyForPath(path, kiln));
      } catch {
        // Unindexed or brand-new note: an empty panel, not an error toast.
        return null;
      }
    },
  );

  /** Wikilink identities of the focused note (stem + kiln-relative path)
   * that a linking note's wikilinks can point at. */
  const focusedKeys = createMemo(() => {
    const path = focusedFile();
    if (!path) return [];
    const rel = noteKeyForPath(path, kilnPath() ?? null).replace(/\.md$/, '');
    const stem = (path.split('/').pop() ?? path).replace(/\.md$/, '');
    return [...new Set([rel, stem])];
  });

  // Referencing block per linked note: fetch each linking note (capped) and
  // pull the first line whose wikilink targets the focused note. Purely
  // additive context — entries render fine while (or if never) resolved.
  const SNIPPET_CAP = 25;
  const [linkingBlocks] = createResource(
    () => {
      const data = backlinks();
      const keys = focusedKeys();
      if (!data || data.linked.length === 0 || keys.length === 0) return null;
      return { linked: data.linked.slice(0, SNIPPET_CAP), keys };
    },
    async ({ linked, keys }) => {
      const out: Record<string, LinkingBlock> = {};
      await Promise.all(
        linked.map(async (entry) => {
          try {
            const content = await getFileContent(entry.abs_path);
            // The daemon's link index gives the exact byte span of the
            // occurrence; the regex scan is the legacy-row fallback.
            const block =
              (entry.span_start != null ? blockAtByteOffset(content, entry.span_start) : null) ??
              findLinkingBlock(content, keys);
            if (block) out[entry.abs_path] = block;
          } catch {
            // Unreadable linking note: row simply shows no snippet.
          }
        }),
      );
      return out;
    },
  );

  const visibleUnlinked = createMemo(() => {
    const data = backlinks();
    if (!data) return [];
    const hidden = dismissed();
    return data.unlinked.filter((s) => !hidden.has(suggestionKey(s)));
  });

  const applySuggestion = (s: UnlinkedMention) => {
    const path = focusedFile();
    if (!path) return;
    const file = editor.openFiles().find((f) => f.path === path);
    if (!file) return;

    const updated = insertWikilink(file.content, s);
    if (updated === null) {
      notificationActions.addNotification(
        'warning',
        `Couldn't locate "${s.mention}" — the note changed since suggestions were computed`,
      );
    } else {
      editor.updateFileContent(path, updated);
    }
    setDismissed((prev) => new Set(prev).add(suggestionKey(s)));
  };

  return (
    <PanelShell>
      <PanelHeader title="Backlinks" class="shrink-0">
        <div class="mt-1 flex items-center justify-between gap-2">
          <span class="truncate text-xs text-muted-dark" data-testid="backlinks-note-title">
            {backlinks()?.note.title || (focusedFile() ? focusedFile()!.split('/').pop() : '')}
          </span>
          <Show when={focusedFile()}>
            <button
              type="button"
              data-testid="backlinks-refresh"
              class="rounded p-1 text-muted-dark hover:bg-hover-wash hover:text-shell-body"
              title="Refresh backlinks"
              onClick={() => setRefreshTick((t) => t + 1)}
            >
              <RefreshCw class="h-3.5 w-3.5" />
            </button>
          </Show>
        </div>
      </PanelHeader>

      <div class="flex-1 overflow-y-auto p-2" data-testid="backlinks-panel">
        <Show
          when={focusedFile()}
          fallback={
            <div class="px-2 py-4 text-sm text-muted-dark" data-testid="backlinks-empty">
              Open a note to see its backlinks.
            </div>
          }
        >
          {/* Linked mentions — incoming wikilink edges */}
          <div class="mb-1 px-2 pt-1 text-[11px] font-semibold uppercase tracking-wide text-muted-dark">
            Linked mentions ({backlinks()?.linked.length ?? 0})
          </div>
          <Show
            when={(backlinks()?.linked.length ?? 0) > 0}
            fallback={
              <div class="px-2 pb-2 text-xs text-muted-dark">
                No notes link here yet.
              </div>
            }
          >
            <For each={backlinks()?.linked}>
              {(entry) => {
                const block = () => linkingBlocks()?.[entry.abs_path];
                return (
                  <button
                    type="button"
                    data-testid="backlinks-linked-item"
                    data-note={entry.name}
                    // Hover previews of this row scroll to the wikilink that
                    // points back at the focused note (exact line when the
                    // link index resolved one; note-match fallback).
                    data-scroll-note={focusedKeys()[0] ?? ''}
                    data-scroll-line={block()?.line ?? ''}
                    class="block w-full rounded px-2 py-1.5 text-left hover:bg-hover-wash"
                    onClick={() =>
                      // Global open event: the app routes it to the window-tab
                      // editor; harnesses route it to their own EditorContext.
                      window.dispatchEvent(
                        new CustomEvent('crucible:open-file', {
                          detail: { path: entry.abs_path, name: entry.name },
                        }),
                      )
                    }
                  >
                    <span class="block truncate text-sm text-shell-ink">
                      {entry.title || entry.name}
                    </span>
                    <span class="block truncate text-[11px] text-muted-dark">{entry.path}</span>
                    <Show when={block()}>
                      {(b) => (
                        <span
                          data-testid="backlinks-snippet"
                          class="mt-1 block border-l-2 border-hairline-strong pl-2 text-[11px] leading-snug text-muted line-clamp-2"
                        >
                          {b().snippet}
                        </span>
                      )}
                    </Show>
                  </button>
                );
              }}
            </For>
          </Show>

          {/* Unlinked mentions — plain-text references in this note */}
          <div class="mb-1 mt-3 px-2 text-[11px] font-semibold uppercase tracking-wide text-muted-dark">
            Unlinked mentions in this note ({visibleUnlinked().length})
          </div>
          <Show
            when={visibleUnlinked().length > 0}
            fallback={
              <div class="px-2 pb-2 text-xs text-muted-dark">
                No unlinked mentions found.
              </div>
            }
          >
            <For each={visibleUnlinked()}>
              {(s) => (
                <div
                  data-testid="backlinks-unlinked-item"
                  class="flex items-center justify-between gap-2 rounded px-2 py-1.5 hover:bg-hover-wash"
                >
                  <div class="min-w-0">
                    <span class="block truncate text-sm text-shell-body">“{s.mention}”</span>
                    <span class="block truncate text-[11px] text-muted-dark" data-note={s.target}>
                      → {s.target}
                    </span>
                  </div>
                  <button
                    type="button"
                    data-testid="backlinks-link-button"
                    class="shrink-0 rounded border border-hairline px-2 py-0.5 text-xs text-primary hover:bg-hover-wash"
                    title={`Convert to [[${s.target}]]`}
                    onClick={() => applySuggestion(s)}
                  >
                    Link
                  </button>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </div>
    </PanelShell>
  );
};
