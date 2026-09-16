import { Component, For, Show, createMemo, createSignal, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useWorkspaceTargetsByRoot } from '@/lib/query/targets';
import type { Session } from '@/lib/types';
import { sessionDefaultKiln, sessionWorkspace } from '@/lib/session-scope';
import { PanelShell } from './PanelShell';
import { TreeSection } from '@/components/tree/TreeSection';
import { byRecency, inboxSessions } from '@/lib/session-inbox';
import { reflectionSessions } from '@/lib/session-reflections';
import { SessionRow, SessionTree } from './SessionTree';
import { EmptyState } from '@/components/ui/EmptyState';
import { ProjectMenu } from '@/components/shell/ProjectMenu';

/**
 * The sessions rail — an Inbox of the last few sessions, then two tiers,
 * project over session, with Reflections and Archived collapsibles below.
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

  const [showArchived, setShowArchived] = createSignal(false);
  const [reflectionsOpen, setReflectionsOpen] = createSignal(false);

  onMount(() => {
    refreshSessions({ includeArchived: true });
  });

  // Live branch per checkout path, from the workspace provider that owns the
  // concept. One query per repo root, shared with every other reader of the
  // same root: this used to re-run the whole fan-out on every roster change
  // and again on every window focus, which cost N plugin commands each time.
  // Nothing refetches on focus now — `queryClientOptions` turns that off for
  // every query, because the daemon pushes a change over SSE instead.
  const repoRoots = createMemo(() => [
    ...new Set(projects().filter((p) => p.repository?.root).map((p) => p.repository!.root)),
  ]);
  const targetsByRoot = useWorkspaceTargetsByRoot(repoRoots);
  const checkoutBranch = createMemo(() => {
    const map = new Map<string, string>();
    for (const targets of targetsByRoot().values()) {
      for (const t of targets) if (t.path) map.set(t.path, t.label);
    }
    return map;
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
   * The last few sessions the user touched, whatever their project. The tree
   * draws it above its project tier and does not repeat its rows, so one
   * session has one row on the rail; it moves down into its project when
   * newer work pushes it out.
   */
  const inbox = createMemo(() => inboxSessions(activeList()));
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

  /**
   * The tree below leaves the passes to that section, so neither repeats.
   * Inbox members stay in this list: the tree needs them to know which
   * projects have sessions, and it draws them once, in its Inbox.
   */
  const treeList = createMemo(() => activeList().filter((s) => s.type !== 'plugin'));

  const row = (s: Session) => (
    <SessionRow
      session={s}
      selected={currentSession()?.session_id === s.session_id}
      branch={branchOfSession(s)}
      kilnLabel={kilnName(sessionDefaultKiln(s))}
      onSelect={() => selectSession(s.session_id)}
      onArchive={() => archiveSession(s.session_id)}
      onDelete={() => deleteSession(s.session_id)}
    />
  );

  /** Start a session in one project. The tree offers it per group row. */
  const newSessionIn = (projectPath: string) =>
    window.dispatchEvent(
      new CustomEvent('crucible:new-session', { detail: { workspace: projectPath } }),
    );

  // Created here, outside the tree's own context menu: an ark Menu.Root that
  // mounts inside another becomes its submenu.
  const projectMenu = <ProjectMenu />;

  return (
    <PanelShell>
      {/* The whole rail is the session list: Inbox, tree, Reflections and
          Archived are its sections. */}
      <div class="flex-1 overflow-y-auto px-1 py-1.5 flex flex-col gap-2" data-testid="session-list">
        <SessionTree
          sessions={treeList()}
          inbox={inbox()}
          projectsActions={projectMenu}
          currentSessionId={currentSession()?.session_id}
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
        {/* Projects alone put nothing on the rail now — a project shows once
            a session starts in it — so no session is the empty state, whether
            or not the registry has entries. */}
        <Show when={!treeList().length}>
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
