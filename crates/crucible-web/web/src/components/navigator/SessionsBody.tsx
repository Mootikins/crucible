import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { listKilns, scmBranches } from '@/lib/api';
import type { KilnListEntry, Session } from '@/lib/types';
import { kilnLabel } from '@/lib/kiln-label';
import { sessionDisplayTitle } from '@/lib/session-display';
import { SessionRow } from '../SessionTree';
import { Search, Plus, ChevronRight, X } from '@/lib/icons';

const byRecency = (a: Session, b: Session) =>
  (Date.parse(b.last_activity ?? b.started_at) || 0) - (Date.parse(a.last_activity ?? a.started_at) || 0);

/**
 * Flat sessions list for the Navigator's "Sessions" scope: recency-ordered,
 * with an Archived collapsible. No project grouping / facet chips (that was the
 * old standalone panel) — a session carries its kiln + branch on its own row.
 */
export const SessionsBody: Component = () => {
  const { currentSession, sessions, selectSession, archiveSession, deleteSession, refreshSessions } = useSessionSafe();
  const { projects } = useProjectSafe();

  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [checkoutBranch, setCheckoutBranch] = createSignal<Map<string, string>>(new Map());
  const [q, setQ] = createSignal('');
  const [showArchived, setShowArchived] = createSignal(false);

  onMount(() => {
    void listKilns().then(setKilns).catch(() => {});
    refreshSessions({ includeArchived: true });
  });

  // Live branch per checkout path (one scm.branches call per repo root).
  const loadBranches = async () => {
    const roots = [...new Set(projects().filter((p) => p.repository?.root).map((p) => p.repository!.root))];
    const map = new Map<string, string>();
    await Promise.all(
      roots.map(async (root) => {
        try {
          const res = await scmBranches(root);
          for (const b of res.branches) if (b.worktree_path) map.set(b.worktree_path, b.name);
        } catch { /* older daemon / repo gone */ }
      }),
    );
    setCheckoutBranch(map);
  };
  createEffect(() => { projects(); void loadBranches(); });
  onMount(() => {
    const onFocus = () => void loadBranches();
    window.addEventListener('focus', onFocus);
    onCleanup(() => window.removeEventListener('focus', onFocus));
  });

  const branchOf = (workspace: string): string | null => {
    const map = checkoutBranch();
    const direct = map.get(workspace);
    if (direct) return direct;
    for (const [checkout, branch] of map) if (workspace.startsWith(checkout + '/')) return branch;
    return null;
  };
  const kilnName = (path: string): string | null =>
    path ? kilnLabel(path, kilns().find((k) => k.path === path)?.name) : null;

  const matches = (s: Session) => {
    const query = q().trim().toLowerCase();
    return !query || (sessionDisplayTitle(s) || '').toLowerCase().includes(query);
  };
  const activeList = createMemo(() => sessions().filter((s) => !s.archived && matches(s)).sort(byRecency));
  const archivedList = createMemo(() => sessions().filter((s) => s.archived && matches(s)).sort(byRecency));

  const row = (s: Session) => (
    <SessionRow
      session={s}
      selected={currentSession()?.id === s.id}
      branch={s.workspace && s.workspace !== s.kiln ? branchOf(s.workspace) : null}
      kilnLabel={kilnName(s.kiln)}
      onSelect={() => selectSession(s.id)}
      onArchive={() => archiveSession(s.id)}
      onDelete={() => deleteSession(s.id)}
    />
  );

  return (
    <div class="flex flex-col h-full">
      <div class="p-2.5 shrink-0 flex flex-col gap-2">
        <div class="relative">
          <Search class="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-dark" />
          <input
            type="text"
            value={q()}
            onInput={(e) => setQ(e.currentTarget.value)}
            placeholder="Search sessions…"
            class="w-full bg-control text-shell-ink text-sm pl-8 pr-7 py-1.5 rounded border border-hairline focus:border-primary focus:outline-none placeholder:text-muted-dark"
            data-testid="session-search-input"
          />
          <Show when={q()}>
            <button onClick={() => setQ('')} class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-muted-dark hover:text-shell-body rounded">
              <X class="w-3 h-3" />
            </button>
          </Show>
        </div>
        <button
          onClick={() => window.dispatchEvent(new CustomEvent('crucible:new-session'))}
          class="w-full px-3 py-2 text-sm text-muted hover:text-shell-ink hover:bg-hover-wash rounded-lg transition-colors flex items-center justify-center gap-2"
          data-testid="new-session-button"
        >
          <Plus class="w-3.5 h-3.5" /> New Session
        </button>
      </div>

      <div class="flex-1 overflow-y-auto px-1 pb-2" data-testid="session-list">
        <div class="flex flex-col gap-0.5">
          <For each={activeList()}>{row}</For>
        </div>
        <Show when={!activeList().length && !q()}>
          <p class="px-3 py-6 text-center text-muted-dark text-sm">No sessions yet</p>
        </Show>
        <Show when={archivedList().length}>
          <button
            onClick={() => setShowArchived((v) => !v)}
            class="w-full flex items-center gap-1 px-3 pt-3 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-dark hover:text-shell-body"
          >
            <ChevronRight class="w-3 h-3 transition-transform" style={{ transform: showArchived() ? 'rotate(90deg)' : 'none' }} />
            Archived · {archivedList().length}
          </button>
          <Show when={showArchived()}>
            <div class="opacity-60 flex flex-col gap-0.5"><For each={archivedList()}>{row}</For></div>
          </Show>
        </Show>
      </div>
    </div>
  );
};
