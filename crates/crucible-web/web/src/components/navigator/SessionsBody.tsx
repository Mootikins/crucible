import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { listWorkspaceTargets } from '@/lib/api';
import type { Session } from '@/lib/types';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
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

  const [checkoutBranch, setCheckoutBranch] = createSignal<Map<string, string>>(new Map());
  const [showArchived, setShowArchived] = createSignal(false);

  onMount(() => {
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

  /** The live branch for a session's workspace, or null when it has none. */
  const branchOfSession = (s: Session): string | null => {
    const workspace = sessionWorkspace(s);
    return workspace ? branchOf(workspace) : null;
  };
  const branchOf = (workspace: string): string | null => {
    const map = checkoutBranch();
    const direct = map.get(workspace);
    if (direct) return direct;
    for (const [checkout, branch] of map) if (workspace.startsWith(checkout + '/')) return branch;
    return null;
  };
  // Takes a session's kiln NAME or `null`, never ''.
  //
  // `sessionDefaultKiln` returns a registry name, so this used to join it
  // against `k.path` — a lookup that never matched, and whose only effect was
  // to send the name through `kilnLabel`'s basename fallback. A valid
  // `KilnName` has no separators, so the fallback returned it unchanged and the
  // chip looked right by coincidence; a kiln legitimately named `.crucible`
  // rendered as "Home kiln".
  //
  // The name IS the label, so nothing is looked up and nothing can be borrowed
  // from a neighbouring kiln. The `kiln.list` fetch this used to need is gone
  // with it.
  const kilnName = (name: string | null): string | null => name || null;

  const activeList = createMemo(() => sessions().filter((s) => !s.archived).sort(byRecency));
  const archivedList = createMemo(() => sessions().filter((s) => s.archived).sort(byRecency));

  const row = (s: Session) => (
    <SessionRow
      session={s}
      selected={currentSession()?.id === s.id}
      branch={branchOfSession(s)}
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
