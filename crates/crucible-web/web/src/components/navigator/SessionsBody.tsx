import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { listKilns, listWorkspaceTargets } from '@/lib/api';
import type { KilnListEntry, Session } from '@/lib/types';
import { kilnLabel } from '@/lib/kiln-label';
import { sessionDefaultKiln, sessionHasWorkspace } from '@/lib/session-scope';
import { SessionRow } from '../SessionTree';
import { Plus, ChevronRight } from '@/lib/icons';

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
  const [showArchived, setShowArchived] = createSignal(false);

  onMount(() => {
    void listKilns().then(setKilns).catch(() => {});
    refreshSessions({ includeArchived: true });
  });

  // Live branch per checkout path, from the workspace provider that owns the
  // concept (one call per repo root).
  const loadBranches = async () => {
    const roots = [...new Set(projects().filter((p) => p.repository?.root).map((p) => p.repository!.root))];
    const map = new Map<string, string>();
    await Promise.all(
      roots.map(async (root) => {
        for (const t of await listWorkspaceTargets(root)) if (t.path) map.set(t.path, t.label);
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
  // Takes the kiln or `null`, never '': the empty path resolves to the home
  // data dir, so a kiln-less session would be labelled with a kiln it has not
  // attached.
  const kilnName = (path: string | null): string | null =>
    path ? kilnLabel(path, kilns().find((k) => k.path === path)?.name) : null;

  const activeList = createMemo(() => sessions().filter((s) => !s.archived).sort(byRecency));
  const archivedList = createMemo(() => sessions().filter((s) => s.archived).sort(byRecency));

  const row = (s: Session) => (
    <SessionRow
      session={s}
      selected={currentSession()?.id === s.id}
      branch={sessionHasWorkspace(s) ? branchOf(s.workspace) : null}
      kilnLabel={kilnName(sessionDefaultKiln(s))}
      onSelect={() => selectSession(s.id)}
      onArchive={() => archiveSession(s.id)}
      onDelete={() => deleteSession(s.id)}
    />
  );

  return (
    <div class="flex flex-col h-full">
      <div class="p-2.5 shrink-0">
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
        <Show when={!activeList().length}>
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
