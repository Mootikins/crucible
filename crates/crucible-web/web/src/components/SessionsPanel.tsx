import { Component, For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { listWorkspaceTargets } from '@/lib/api';
import type { Session } from '@/lib/types';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import { PanelShell } from './PanelShell';
import { TreeSection } from '@/components/tree/TreeSection';
import { sessionStatus } from '@/lib/session-status';
import { inboxSessions } from '@/lib/session-inbox';
import { reflectionSessions } from '@/lib/session-reflections';
import { SessionRow, SessionTree } from './SessionTree';
import { EmptyState } from '@/components/ui/EmptyState';

const byRecency = (a: Session, b: Session) =>
  (Date.parse(b.last_activity ?? b.started_at) || 0) - (Date.parse(a.last_activity ?? a.started_at) || 0);

/**
 * The sessions rail — two tiers, project over session, with an Archived
 * collapsible below.
 *
 * Its own panel, NOT a scope of the file tree. The Navigator made the two
 * mutually exclusive, so reading a file hid the session list and switching
 * session context cost a scope change; they now sit on opposite rails and are
 * both visible at once.
 *
 * The project tier is not decoration. A session belongs to exactly one
 * project, so the project is where "start a session" belongs: each group row
 * carries its own New Session, on hover and in its context menu. The
 * panel-wide button they replace could not name the project it meant, and the
 * flat recency list this panel used to render could not either — it left the
 * grouping to `SessionTree`, which nothing rendered.
 */
export const SessionsPanel: Component = () => {
  const { currentSession, sessions, selectSession, archiveSession, deleteSession, refreshSessions } = useSessionSafe();
  const { projects, currentProject, selectProject } = useProjectSafe();

  const [checkoutBranch, setCheckoutBranch] = createSignal<Map<string, string>>(new Map());
  const [showArchived, setShowArchived] = createSignal(false);
  const [inboxOpen, setInboxOpen] = createSignal(true);
  const [reflectionsOpen, setReflectionsOpen] = createSignal(false);

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

  /**
   * The Inbox — see `lib/session-inbox.ts`, which both shells share.
   *
   * Membership is "not idle AND touched in the last day". The staleness rule
   * is what keeps it an inbox rather than a second session list — an agent
   * that has been blocked on a question since last week is not news, and left
   * in, it would sit at the top of the rail forever. It stays reachable in the
   * tree below, under its own project.
   */
  const inbox = createMemo(() => inboxSessions(activeList()));
  const waitingCount = () => inbox().filter((s) => sessionStatus(s) === 'waiting').length;
  const archivedList = createMemo(() => sessions().filter((s) => s.archived).sort(byRecency));

  /**
   * The passes a plugin ran for itself — see `lib/session-reflections.ts`.
   *
   * Its own section because a pass has no workspace, so the tree below files
   * it under "No project" with everything else that has none. The daemon
   * keeps a pass out of the archive while its review queue is undecided, and
   * this is where the user goes to decide it.
   */
  const reflections = createMemo(() => reflectionSessions(sessions()));

  /** The tree below leaves the passes to that section, so neither repeats. */
  const treeList = createMemo(() => activeList().filter((s) => s.session_type !== 'plugin'));

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

  /** Start a session in one project. The tree offers it per group row. */
  const newSessionIn = (projectPath: string) =>
    window.dispatchEvent(
      new CustomEvent('crucible:new-session', { detail: { workspace: projectPath } }),
    );

  return (
    <PanelShell>
      {/* No switcher header. It was a dropdown listing sessions, sitting on
          top of a list of sessions — its one unique offer was a GLOBAL
          "active" group, and the Inbox below is that group, in the open,
          without a click. The waiting count rides the Inbox header instead. */}
      <div class="flex-1 overflow-y-auto px-1 py-1.5">
        {/* Above the tree, because it is what you came to look at. */}
        <TreeSection
          label="Inbox"
          count={inbox().length}
          open={inboxOpen()}
          onToggle={() => setInboxOpen((v) => !v)}
          testid="inbox-section"
          urgent={waitingCount() > 0}
        >
          <div class="flex flex-col">
            <For each={inbox()}>{row}</For>
          </div>
        </TreeSection>

        <SessionTree
          sessions={treeList()}
          currentSessionId={currentSession()?.id}
          projects={projects()}
          currentProjectPath={currentProject()?.path}
          onSelectSession={(id) => void selectSession(id)}
          onSelectProject={(path) => void selectProject(path)}
          onNewSession={newSessionIn}
          onArchiveSession={(id) => void archiveSession(id)}
          onDeleteSession={(id) => void deleteSession(id)}
          branchOf={branchOf}
          kilnName={kilnName}
        />
        <Show when={!projects().length && !treeList().length}>
          <EmptyState
            title="No sessions yet"
            body="Start one to give an agent a workspace and a kiln."
            // No workspace in the detail: there is no project to name, so
            // the draft asks for one. The palette dispatches the same event.
            action={{
              label: 'New session',
              onClick: () => window.dispatchEvent(new CustomEvent('crucible:new-session')),
            }}
            testid="sessions-empty"
          />
        </Show>
        <TreeSection
          label="Reflections"
          count={reflections().length}
          open={reflectionsOpen()}
          onToggle={() => setReflectionsOpen((v) => !v)}
          testid="reflections-section"
        >
          <div class="flex flex-col">
            <For each={reflections()}>{row}</For>
          </div>
        </TreeSection>
        <TreeSection
          label="Archived"
          count={archivedList().length}
          open={showArchived()}
          onToggle={() => setShowArchived((v) => !v)}
          testid="archived-section"
        >
          <div class="opacity-60 flex flex-col">
            <For each={archivedList()}>{row}</For>
          </div>
        </TreeSection>
      </div>
    </PanelShell>
  );
};
