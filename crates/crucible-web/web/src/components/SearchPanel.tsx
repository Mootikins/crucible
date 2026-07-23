import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from 'solid-js';
import { PanelShell } from './PanelShell';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import { getConfig, grepSearch, listKilns, searchSessions, type GrepHit } from '@/lib/api';
import type { KilnListEntry, Session } from '@/lib/types';
import { swrLocal } from '@/lib/local-cache';
import { openFileInEditor } from '@/lib/file-actions';
import { pathBasename } from '@/stores/statusBarStore';
import { kilnLabel } from '@/lib/kiln-label';
import { relativeTime } from '@/lib/format-time';
import { treeSectionHeader } from '@/components/tree/tree-style';
import { Search, FileText, FolderGit2, FlaskConical, ClipboardList, X } from '@/lib/icons';

type Source = 'notes' | 'files' | 'sessions';
const ALL_SOURCES: Source[] = ['notes', 'files', 'sessions'];
const HIT_LIMIT = 60;
const DEBOUNCE_MS = 220;

/** Split a line into [before, match, after] for <mark> highlighting without
 * innerHTML — offsets come from the daemon (char positions into `text`). */
function highlightParts(hit: GrepHit): [string, string, string] {
  const s = Math.max(0, Math.min(hit.matchStart, hit.text.length));
  const e = Math.max(s, Math.min(hit.matchEnd, hit.text.length));
  return [hit.text.slice(0, s), hit.text.slice(s, e), hit.text.slice(e)];
}

const HitRow: Component<{ hit: GrepHit; onOpen: () => void }> = (props) => {
  const parts = createMemo(() => highlightParts(props.hit));
  return (
    <button
      type="button"
      onClick={props.onOpen}
      title={`${props.hit.relPath}:${props.hit.line}`}
      class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors group/hit"
      data-testid="search-hit"
    >
      <div class="flex items-center gap-1.5 min-w-0">
        <FileText class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
        <span class="text-xs text-shell-body truncate">{pathBasename(props.hit.relPath)}</span>
        <span class="text-[10px] text-muted-dark shrink-0">:{props.hit.line}</span>
        <span class="text-[10px] text-muted-dark truncate ml-auto pl-2">{props.hit.relPath}</span>
      </div>
      <div class="mt-0.5 pl-5 text-[11px] font-mono leading-snug text-muted whitespace-pre-wrap break-all line-clamp-2">
        {parts()[0]}
        <mark class="bg-primary/25 text-shell-ink rounded-sm">{parts()[1]}</mark>
        {parts()[2]}
      </div>
    </button>
  );
};

/**
 * Unified content search: notes (ripgrep over the primary kiln's .md files),
 * project files (ripgrep over the active project), and sessions (title/content).
 * One debounced query fans out to all three sources; each renders under its own
 * section with a count, and the facet chips toggle which sources show.
 */
export const SearchPanel: Component = () => {
  const { projects } = useProjectSafe();
  const { selectSession } = useSessionSafe();

  const [kilnPath, setKilnPath] = createSignal('');
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [query, setQuery] = createSignal('');
  const [debounced, setDebounced] = createSignal('');
  const [active, setActive] = createSignal<Set<Source>>(new Set(ALL_SOURCES));

  const [noteHits, setNoteHits] = createSignal<GrepHit[]>([]);
  const [fileHits, setFileHits] = createSignal<GrepHit[]>([]);
  const [sessionHits, setSessionHits] = createSignal<Session[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let inputRef: HTMLInputElement | undefined;

  // The primary kiln (notes corpus) and the most-recently-used project (files
  // corpus). The daemon sorts projects by last_accessed, so [0] is the MRU.
  const projectRoot = createMemo(() => projects()[0]?.path ?? '');
  const projectName = createMemo(() => {
    const p = projects()[0];
    return p ? p.name || pathBasename(p.path) : '';
  });
  const kilnName = createMemo(() => {
    const k = kilns().find((x) => x.path === kilnPath());
    return kilnLabel(kilnPath(), k?.name);
  });

  onMount(() => {
    swrLocal('config', getConfig, (cfg) => cfg?.kiln_path && setKilnPath(cfg.kiln_path));
    swrLocal('kilns', listKilns, setKilns);
    // Focus the box when the panel opens, and when a global "focus search"
    // event fires (Ctrl+Shift+F / palette / ribbon re-open).
    queueMicrotask(() => inputRef?.focus());
    const onFocus = () => {
      inputRef?.focus();
      inputRef?.select();
    };
    window.addEventListener('crucible:focus-search', onFocus);
    onCleanup(() => window.removeEventListener('crucible:focus-search', onFocus));
  });

  // Debounce the query into `debounced`.
  createEffect(
    on(query, (q) => {
      const t = setTimeout(() => setDebounced(q.trim()), DEBOUNCE_MS);
      onCleanup(() => clearTimeout(t));
    }),
  );

  // Run the three searches whenever the debounced query (or corpus) changes.
  // A per-run token drops stale responses (out-of-order guard).
  let runToken = 0;
  createEffect(
    on([debounced, kilnPath, projectRoot], ([q, kiln, proj]) => {
      const token = ++runToken;
      if (!q) {
        setNoteHits([]);
        setFileHits([]);
        setSessionHits([]);
        setError(null);
        setBusy(false);
        return;
      }
      setBusy(true);
      setError(null);
      const guard = <T,>(fn: () => T) => (runToken === token ? fn() : undefined);

      const notes = kiln
        ? grepSearch(kiln, q, { glob: '*.md', limit: HIT_LIMIT })
            .then((r) => guard(() => setNoteHits(r.hits)))
            .catch(() => guard(() => setNoteHits([])))
        : Promise.resolve(guard(() => setNoteHits([])));

      const files = proj
        ? grepSearch(proj, q, { limit: HIT_LIMIT })
            .then((r) => guard(() => setFileHits(r.hits)))
            .catch(() => guard(() => setFileHits([])))
        : Promise.resolve(guard(() => setFileHits([])));

      const sessions = searchSessions(q, undefined, 30)
        .then((r) => guard(() => setSessionHits(r)))
        .catch(() => guard(() => setSessionHits([])));

      void Promise.allSettled([notes, files, sessions]).then(() =>
        guard(() => setBusy(false)),
      );
    }),
  );

  const toggle = (s: Source) =>
    setActive((prev) => {
      const next = new Set(prev);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      // Never allow zero facets — re-enabling all reads clearer than "nothing".
      return next.size === 0 ? new Set(ALL_SOURCES) : next;
    });

  const counts = createMemo(() => ({
    notes: noteHits().length,
    files: fileHits().length,
    sessions: sessionHits().length,
  }));
  const total = () => counts().notes + counts().files + counts().sessions;

  const facet = (s: Source, label: string, icon: Component<{ class?: string }>) => {
    const Icon = icon;
    return (
      <button
        type="button"
        onClick={() => toggle(s)}
        data-testid={`search-facet-${s}`}
        aria-pressed={active().has(s)}
        classList={{
          'inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] border transition-colors': true,
          'border-primary/40 bg-primary/10 text-shell-ink': active().has(s),
          'border-hairline text-muted-dark hover:bg-hover-wash': !active().has(s),
        }}
      >
        <Icon class="w-3 h-3" />
        {label}
        <span class="text-muted-dark">{counts()[s]}</span>
      </button>
    );
  };

  return (
    <PanelShell class="overflow-hidden">
      <div class="p-3 border-b border-hairline shrink-0 flex flex-col gap-2">
        <div class="flex items-center gap-2 bg-surface-base border border-hairline-strong rounded-lg px-2.5 py-1.5 focus-within:border-primary transition-colors">
          <Search class="w-4 h-4 shrink-0 text-muted-dark" />
          <input
            ref={inputRef}
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && query()) {
                e.stopPropagation();
                setQuery('');
              }
            }}
            placeholder="Search notes, files, sessions…"
            aria-label="Search content"
            class="flex-1 min-w-0 bg-transparent text-sm text-shell-ink placeholder-muted-dark outline-none"
            data-testid="search-input"
          />
          <Show when={query()}>
            <button
              type="button"
              onClick={() => {
                setQuery('');
                inputRef?.focus();
              }}
              aria-label="Clear search"
              class="p-0.5 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </Show>
        </div>
        <div class="flex items-center gap-1.5 flex-wrap">
          {facet('notes', 'Notes', FlaskConical)}
          {facet('files', 'Files', FolderGit2)}
          {facet('sessions', 'Sessions', ClipboardList)}
        </div>
        <p class="text-[10px] text-muted-dark truncate">
          <Show when={kilnPath()}>{kilnName()}</Show>
          <Show when={kilnPath() && projectRoot()}> · </Show>
          <Show when={projectRoot()}>{projectName()}</Show>
        </p>
      </div>

      <div class="flex-1 overflow-y-auto py-1" data-testid="search-results">
        <Show when={error()}>
          <div class="mx-3 my-2 px-3 py-2 text-xs text-error bg-error/10 rounded border border-error/30">
            {error()}
          </div>
        </Show>

        <Show when={!debounced()}>
          <div class="px-3 py-8 text-center text-muted-dark text-xs">
            Type to search notes, project files, and sessions.
          </div>
        </Show>

        <Show when={debounced() && !busy() && total() === 0}>
          <div class="px-3 py-8 text-center text-muted-dark text-xs">
            No matches for “{debounced()}”.
          </div>
        </Show>

        {/* Notes */}
        <Show when={active().has('notes') && noteHits().length > 0}>
          <div class={treeSectionHeader}>Notes · {counts().notes}</div>
          <For each={noteHits()}>
            {(hit) => (
              <HitRow hit={hit} onOpen={() => openFileInEditor(hit.path, pathBasename(hit.relPath) || undefined)} />
            )}
          </For>
        </Show>

        {/* Files */}
        <Show when={active().has('files') && fileHits().length > 0}>
          <div class={treeSectionHeader}>Files · {counts().files}</div>
          <For each={fileHits()}>
            {(hit) => (
              <HitRow hit={hit} onOpen={() => openFileInEditor(hit.path, pathBasename(hit.relPath) || undefined)} />
            )}
          </For>
        </Show>

        {/* Sessions */}
        <Show when={active().has('sessions') && sessionHits().length > 0}>
          <div class={treeSectionHeader}>Sessions · {counts().sessions}</div>
          <For each={sessionHits()}>
            {(s) => (
              <button
                type="button"
                onClick={() => selectSession(s.id)}
                title={s.title ?? 'Untitled session'}
                class="w-full text-left px-3 py-1.5 rounded hover:bg-hover-wash transition-colors flex items-center gap-1.5"
                data-testid="search-session-hit"
              >
                <ClipboardList class="w-3.5 h-3.5 shrink-0 text-muted-dark" />
                <span class="text-xs text-shell-body truncate">{s.title ?? 'Untitled session'}</span>
                <Show when={s.started_at}>
                  <span class="text-[10px] text-muted-dark shrink-0 ml-auto pl-2">
                    {relativeTime(s.started_at!)}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </Show>
      </div>
    </PanelShell>
  );
};
