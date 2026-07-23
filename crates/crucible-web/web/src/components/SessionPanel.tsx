import { Component, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSearch } from '@/hooks/useSessionSearch';
import { SessionTree } from './SessionTree';
import { SessionFooter } from './SessionFooter';
import { PanelShell } from './PanelShell';
import { ChipSelect, type ChipOption } from './composer/ChipSelect';
import { isGitRepoUrl, listKilns, scmBranches, scmClone } from '@/lib/api';
import type { KilnListEntry, Session } from '@/lib/types';
import { FlaskConical, GitBranch, Plus, Search, X } from '@/lib/icons';
import { kilnLabel } from '@/lib/kiln-label';
import { btnPrimary, btnNeutral } from '@/lib/button-style';

/**
 * The sessions panel: ONE tree of all sessions grouped by project (worktrees
 * fold into their repo's group), kiln and branch surfaced as row chips and as
 * FACET FILTERS rather than tree levels — filter popouts with checkable
 * entries, per the branch-is-a-filter design call. Recency ordering within
 * and between groups.
 */
export const SessionPanel: Component = () => {
  const { currentProject, projects, selectProject, refreshProjects } =
    useProjectSafe();
  const {
    currentSession,
    sessions,
    isLoading,
    selectSession,
    pauseSession,
    resumeSession,
    refreshSessions,
    providers,
    providersLoaded,
    deleteSession,
    archiveSession,
  } = useSessionSafe();

  const [sessionFilter, setSessionFilter] = createSignal<'active' | 'all' | 'archived'>('active');
  const [selectedBranches, setSelectedBranches] = createSignal<string[]>([]);
  const [selectedKilns, setSelectedKilns] = createSignal<string[]>([]);
  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  // checkout path -> live branch name, from scm.branches per unique repo.
  const [checkoutBranch, setCheckoutBranch] = createSignal<Map<string, string>>(new Map());

  const search = useSessionSearch({
    selectedKiln: () => selectedKilns()[0] ?? '',
    sessions,
    sessionFilter,
  });

  // When session filter changes, re-fetch with appropriate includeArchived flag
  createEffect(() => {
    const filter = sessionFilter();
    const includeArchived = filter === 'all' || filter === 'archived';
    refreshSessions({ includeArchived });
  });

  onMount(() => {
    void listKilns().then(setKilns).catch(() => {});
  });

  // Live branch mapping: one scm.branches call per unique repo root, refreshed
  // when the project roster changes and on window focus (branch switches and
  // new worktrees happen outside the app).
  const loadBranches = async () => {
    const roots = [...new Set(
      projects()
        .filter((p) => p.repository?.root)
        .map((p) => p.repository!.root),
    )];
    const map = new Map<string, string>();
    await Promise.all(
      roots.map(async (root) => {
        try {
          const res = await scmBranches(root);
          for (const b of res.branches) {
            if (b.worktree_path) map.set(b.worktree_path, b.name);
          }
        } catch {
          /* older daemon / repo gone: no branch chips for this repo */
        }
      }),
    );
    setCheckoutBranch(map);
  };

  createEffect(() => {
    projects();
    void loadBranches();
  });

  onMount(() => {
    const onFocus = () => void loadBranches();
    window.addEventListener('focus', onFocus);
    onCleanup(() => window.removeEventListener('focus', onFocus));
  });

  const branchOf = (workspace: string): string | null => {
    const map = checkoutBranch();
    const direct = map.get(workspace);
    if (direct) return direct;
    for (const [checkout, branch] of map) {
      if (workspace.startsWith(checkout + '/')) return branch;
    }
    return null;
  };

  const kilnName = (path: string): string | null => {
    if (!path) return null;
    const match = kilns().find((k) => k.path === path);
    return kilnLabel(path, match?.name);
  };

  const sessionMatchesKilnFacet = (s: Session, facet: string[]) =>
    facet.includes(s.kiln) || s.connected_kilns.some((k) => facet.includes(k));

  const filteredSessions = createMemo(() => {
    let base = search.displayedSessions();
    const branches = selectedBranches();
    if (branches.length > 0) {
      base = base.filter((s) => {
        const b = s.workspace && s.workspace !== s.kiln ? branchOf(s.workspace) : null;
        return b !== null && branches.includes(b);
      });
    }
    const kilnFacet = selectedKilns();
    if (kilnFacet.length > 0) {
      base = base.filter((s) => sessionMatchesKilnFacet(s, kilnFacet));
    }
    return base;
  });

  // Facet options come from the state-filtered (pre-facet) list so adding a
  // second branch/kiln stays possible while one is active.
  const branchOptions = createMemo<ChipOption[]>(() => {
    const counts = new Map<string, number>();
    for (const s of search.displayedSessions()) {
      const b = s.workspace && s.workspace !== s.kiln ? branchOf(s.workspace) : null;
      if (b) counts.set(b, (counts.get(b) ?? 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([name, n]) => ({ value: name, label: name, hint: `${n}` }));
  });

  const kilnOptions = createMemo<ChipOption[]>(() => {
    const counts = new Map<string, number>();
    for (const s of search.displayedSessions()) {
      if (s.kiln) counts.set(s.kiln, (counts.get(s.kiln) ?? 0) + 1);
      for (const k of s.connected_kilns) counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([path, n]) => ({ value: path, label: kilnName(path) ?? path, hint: `${n}` }));
  });

  const toggleIn = (list: string[], value: string) =>
    list.includes(value) ? list.filter((v) => v !== value) : [...list, value];

  const hasFacets = () => selectedBranches().length > 0 || selectedKilns().length > 0;

  // All entry points converge on the draft surface (lazy creation).
  const handleCreateSession = () => {
    window.dispatchEvent(new CustomEvent('crucible:new-session'));
  };

  // Inline add-project: git URLs ONLY from the web (https/ssh or owner/repo
  // shorthand) — clones into `[scm] projects_dir` and registers. Local paths
  // are a CLI/daemon affair; a browser text field is the wrong place to
  // browse the host filesystem.
  const [showNewProject, setShowNewProject] = createSignal(false);
  const [newProjectPath, setNewProjectPath] = createSignal('');
  const [addBusy, setAddBusy] = createSignal(false);
  const [addError, setAddError] = createSignal<string | null>(null);
  const handleRegisterProject = async () => {
    const input = newProjectPath().trim();
    if (!input || addBusy()) return;
    if (!isGitRepoUrl(input)) {
      setAddError('Enter a git URL (https/ssh) or owner/repo');
      return;
    }
    setAddBusy(true);
    setAddError(null);
    try {
      const res = await scmClone(input);
      await refreshProjects();
      selectProject(res.path);
      setNewProjectPath('');
      setShowNewProject(false);
    } catch (err) {
      setAddError(err instanceof Error ? err.message : 'Failed to clone repository');
    } finally {
      setAddBusy(false);
    }
  };

  const session = () => currentSession();
  const createDisabled = createMemo(
    () => isLoading() || (providersLoaded() && providers().length === 0),
  );

  return (
    <PanelShell>
      {/* No "Sessions" heading — the panel tab already names it. */}
      <div class="px-3 pt-2 pb-1 flex flex-col gap-1.5 shrink-0">
        <div class="relative">
          <Search class="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-dark" />
          <input
            ref={search.setSearchInputRef}
            type="text"
            value={search.searchQuery()}
            onInput={(e) => search.handleSearchInput(e.currentTarget.value)}
            placeholder="Search sessions..."
            class="w-full bg-control text-shell-ink text-sm pl-8 pr-7 py-1.5 rounded border border-hairline focus:border-primary focus:outline-none placeholder:text-muted-dark"
            data-testid="session-search-input"
          />
          <Show when={search.searchQuery()}>
            <button
              onClick={search.clearSearch}
              class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-muted-dark hover:text-shell-body rounded"
            >
              <X class="w-3 h-3" />
            </button>
          </Show>
        </div>

        <div class="flex items-center gap-1 flex-wrap">
          <select
            data-testid="session-filter-dropdown"
            value={sessionFilter()}
            onChange={(e) =>
              setSessionFilter(e.target.value as 'active' | 'all' | 'archived')
            }
            class="text-xs bg-surface-elevated text-shell-body border border-hairline rounded px-1 py-0.5"
          >
            <option value="active">Active</option>
            <option value="all">All</option>
            <option value="archived">Archived</option>
          </select>
          <ChipSelect
            name="branch"
            icon={GitBranch}
            multi
            selected={selectedBranches()}
            options={branchOptions()}
            value=""
            onSelect={(v) => setSelectedBranches((prev) => toggleIn(prev, v))}
            testid="session-branch-facet"
          />
          <ChipSelect
            name="kiln"
            icon={FlaskConical}
            multi
            selected={selectedKilns()}
            options={kilnOptions()}
            value=""
            onSelect={(v) => setSelectedKilns((prev) => toggleIn(prev, v))}
            testid="session-kiln-facet"
          />
          <Show when={hasFacets()}>
            <button
              type="button"
              class="text-[11px] text-muted-dark hover:text-shell-ink transition-colors"
              onClick={() => {
                setSelectedBranches([]);
                setSelectedKilns([]);
              }}
              data-testid="session-facet-clear"
            >
              clear
            </button>
          </Show>
        </div>
      </div>

      <div class="flex-1 overflow-y-auto p-2">
        <button
          onClick={handleCreateSession}
          disabled={createDisabled()}
          class="w-full mb-2 px-3 py-2 text-sm text-muted hover:text-shell-ink hover:bg-hover-wash rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
          data-testid="new-session-button"
        >
          <Plus class="w-3.5 h-3.5" />
          New Session
        </button>
        <Show when={providersLoaded() && providers().length === 0}>
          <p class="text-xs text-muted-dark text-center mb-2">No LLM providers detected</p>
        </Show>

        <Show when={search.isSearching()}>
          <p class="text-muted-dark text-sm text-center py-2">Searching...</p>
        </Show>

        <SessionTree
          sessions={filteredSessions()}
          currentSessionId={session()?.id}
          projects={projects()}
          currentProjectPath={currentProject()?.path}
          onSelectSession={selectSession}
          onSelectProject={selectProject}
          onArchiveSession={archiveSession}
          onDeleteSession={deleteSession}
          branchOf={branchOf}
          kilnName={kilnName}
        />

        <Show when={!search.isSearching() && filteredSessions().length === 0}>
          <Show
            when={search.searchQuery().trim()}
            fallback={
              <Show when={hasFacets()}>
                <p class="text-muted-dark text-sm text-center py-2">No sessions match the filters</p>
              </Show>
            }
          >
            <p class="text-muted-dark text-sm text-center py-2">
              No sessions match "{search.searchQuery()}"
            </p>
          </Show>
        </Show>

        <Show when={showNewProject()}>
          <div class="mt-2 p-2 bg-surface-elevated rounded-lg" data-testid="add-project-form">
            <input
              type="text"
              value={newProjectPath()}
              onInput={(e) => setNewProjectPath(e.currentTarget.value)}
              onKeyDown={(e) => e.key === 'Enter' && void handleRegisterProject()}
              placeholder="git URL or owner/repo"
              disabled={addBusy()}
              class="w-full bg-control text-shell-ink px-2 py-1 rounded text-sm placeholder-muted-dark"
              data-testid="add-project-input"
            />
            <Show when={addError()} keyed>
              {(msg) => <p class="mt-1.5 text-xs text-error break-words">{msg}</p>}
            </Show>
            <div class="flex gap-2 mt-2">
              <button
                onClick={() => void handleRegisterProject()}
                disabled={addBusy() || !newProjectPath().trim()}
                class={`flex-1 ${btnPrimary}`}
                data-testid="add-project-submit"
              >
                {addBusy() ? 'Cloning…' : 'Clone'}
              </button>
              <button
                onClick={() => {
                  setShowNewProject(false);
                  setAddError(null);
                }}
                disabled={addBusy()}
                class={btnNeutral}
              >
                Cancel
              </button>
            </div>
          </div>
        </Show>
        <button
          onClick={() => setShowNewProject(true)}
          class="w-full mt-1 px-3 py-1.5 text-xs text-muted-dark hover:text-shell-ink hover:bg-hover-wash rounded-lg transition-colors flex items-center justify-center gap-2"
        >
          <Plus class="w-3 h-3" />
          Add Project
        </button>
      </div>

      <Show when={session()}>
        <SessionFooter
          session={session()!}
          onPause={pauseSession}
          onResume={resumeSession}
          onRefresh={() => refreshSessions()}
        />
      </Show>
    </PanelShell>
  );
};
