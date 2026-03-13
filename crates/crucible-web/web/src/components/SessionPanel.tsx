import { Component, For, Show, createSignal, createEffect, createMemo, onCleanup } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import type { Session, Project, KilnInfo } from '@/lib/types';
import { RefreshCw, Plus, Search, X, Archive, Trash2 } from '@/lib/icons';
import { searchSessions } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';

const KilnSelector: Component<{
  kilns: KilnInfo[];
  selected: string;
  onSelect: (kiln: string) => void;
}> = (props) => {
  const getKilnDisplayName = (kiln: KilnInfo) => {
    // Use the kiln name if available, otherwise fall back to last path segment
    if (kiln.name) {
      return kiln.name;
    }
    const parts = kiln.path.split('/');
    return parts[parts.length - 1] || kiln.path;
  };

  return (
    <Show when={props.kilns.length > 0}>
      <div class="px-3 py-2 border-b border-neutral-800">
        <label class="text-xs text-neutral-500 block mb-1">Kiln</label>
        <Show
          when={props.kilns.length > 1}
          fallback={
            <div class="text-sm text-neutral-300 truncate" title={props.kilns[0].path}>
              {getKilnDisplayName(props.kilns[0])}
            </div>
          }
        >
          <select
            value={props.selected}
            onChange={(e) => props.onSelect(e.currentTarget.value)}
            class="w-full bg-surface-elevated text-neutral-200 text-sm px-2 py-1.5 rounded border border-neutral-700 focus:border-primary focus:outline-none"
          >
            <For each={props.kilns}>
              {(kiln) => (
                <option value={kiln.path}>{getKilnDisplayName(kiln)}</option>
              )}
            </For>
          </select>
        </Show>
      </div>
    </Show>
  );
};

const StateIndicator: Component<{ state: Session['state'] }> = (props) => {
  const colorClass = () => {
    switch (props.state) {
      case 'active': return 'bg-green-500';
      case 'paused': return 'bg-yellow-500';
      case 'compacting': return 'bg-blue-500';
      case 'ended': return 'bg-neutral-500';
      default: return 'bg-neutral-500';
    }
  };

  return (
    <span class={`inline-block w-2 h-2 rounded-full ${colorClass()}`} title={props.state} />
  );
};

const ProjectItem: Component<{ project: Project; selected: boolean; onSelect: () => void }> = (props) => {
   return (
     <button
       onClick={props.onSelect}
       class={`w-full text-left px-3 py-2 rounded-lg transition-colors ${
         props.selected
           ? 'bg-primary/20 text-primary-hover'
           : 'hover:bg-surface-elevated text-neutral-300'
       }`}
     >
      <div class="font-medium truncate">{props.project.name}</div>
      <div class="text-xs text-neutral-500 truncate">{props.project.path}</div>
    </button>
  );
};

const SessionItem: Component<{
  session: Session;
  selected: boolean;
  onSelect: () => void;
  onArchive?: () => void;
  onDelete?: () => void;
}> = (props) => {
   return (
     <div
       onClick={props.onSelect}
      class={`group relative w-full text-left px-3 py-2 rounded-lg transition-colors cursor-pointer ${
        props.selected
          ? 'bg-primary/20 text-primary-hover'
          : 'hover:bg-surface-elevated text-neutral-300'
      }`}
       data-testid={`session-item-${props.session.id}`}
     >
      <div class="flex items-center gap-2">
        <StateIndicator state={props.session.state} />
        <span class="font-medium truncate flex-1">
          {props.session.title || `Session ${props.session.id?.slice(0, 8) ?? 'unknown'}`}
        </span>
      </div>
      <div class="text-xs text-neutral-500 mt-1">
        {props.session.agent_model || 'No model'}
      </div>

      {/* Action buttons — visible on hover */}
      <div class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
        <Show when={props.onArchive}>
          <button
            type="button"
            class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
            title="Archive session"
            onClick={(e) => { e.stopPropagation(); props.onArchive?.(); }}
          >
            <Archive size={14} />
          </button>
        </Show>
        <Show when={props.onDelete}>
          <button
            type="button"
            class="rounded p-1 text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700/60 transition-colors"
            title="Delete session"
            onClick={(e) => { e.stopPropagation(); props.onDelete?.(); }}
          >
            <Trash2 size={14} />
          </button>
        </Show>
      </div>
    </div>
  );
};

export const SessionPanel: Component = () => {
  const { currentProject, projects, selectProject, registerProject } = useProjectSafe();
  const {
    currentSession,
    sessions,
    isLoading,
    createSession,
    selectSession,
    pauseSession,
    resumeSession,
    // endSession,
    refreshSessions,
    selectedProvider,
    providers,
    deleteSession,
    archiveSession,
  } = useSessionSafe();

  const [showNewProject, setShowNewProject] = createSignal(false);
  const [newProjectPath, setNewProjectPath] = createSignal('');
  const [selectedKiln, setSelectedKiln] = createSignal<string>('');
  const [searchQuery, setSearchQuery] = createSignal('');
  const [sessionFilter, setSessionFilter] = createSignal<'active' | 'all' | 'archived'>('active');
  const [searchResults, setSearchResults] = createSignal<Session[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchInputRef: HTMLInputElement | undefined;

  const performSearch = async (query: string) => {
    if (!query.trim()) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    try {
      const results = await searchSessions(query, selectedKiln() || undefined, 20);
      setSearchResults(results);
    } catch (err) {
      console.error('Session search failed:', err);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  };

  const handleSearchInput = (value: string) => {
    setSearchQuery(value);
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => performSearch(value), 300);
  };

  const clearSearch = () => {
    setSearchQuery('');
    setSearchResults([]);
    setIsSearching(false);
    if (searchTimer) clearTimeout(searchTimer);
  };

  /** Focus the search input (called from command palette). */
  const focusSearchInput = () => {
    searchInputRef?.focus();
  };

  // Expose focus method via custom event for command palette wiring
  const onFocusSearch = () => focusSearchInput();
  window.addEventListener('crucible:focus-session-search', onFocusSearch);

  onCleanup(() => {
    if (searchTimer) clearTimeout(searchTimer);
    window.removeEventListener('crucible:focus-session-search', onFocusSearch);
  });

  const displayedSessions = () => {
    const base = searchQuery().trim() ? searchResults() : sessions();
    const filter = sessionFilter();
    if (filter === 'active') {
      return base.filter(s => !s.archived && s.state !== 'ended');
    }
    if (filter === 'archived') {
      return base.filter(s => s.archived === true);
    }
    return base; // 'all'
  };

  // When session filter changes, re-fetch with appropriate includeArchived flag
  createEffect(() => {
    const filter = sessionFilter();
    const includeArchived = filter === 'all' || filter === 'archived';
    refreshSessions({ includeArchived });
  });

  createEffect(() => {
    const project = currentProject();
    if (project && project.kilns.length > 0) {
      if (!selectedKiln() || !project.kilns.some((k) => k.path === selectedKiln())) {
        setSelectedKiln(project.kilns[0].path);
      }
    }
  });

  const handleCreateSession = async () => {
    const project = currentProject();
    const kiln = selectedKiln();
    if (!project) {
      notificationActions.addNotification('error', 'Select a project before creating a session');
      return;
    }
    if (!kiln) {
      notificationActions.addNotification('error', 'Select a kiln before creating a session');
      return;
    }
    if (providers().length === 0) {
      notificationActions.addNotification('error', 'No LLM providers available. Configure a provider first.');
      return;
    }

    const provider = selectedProvider();

    await createSession({
      kiln,
      workspace: project.path,
      provider: provider?.provider_type ?? 'ollama',
      model: provider?.default_model ?? 'llama3.2',
      endpoint: provider?.endpoint,
    });
  };

  const handleKilnSelect = async (kiln: string) => {
    setSelectedKiln(kiln);
    await refreshSessions({ kiln, workspace: currentProject()?.path });
  };

  const handleRegisterProject = async () => {
    const path = newProjectPath().trim();
    if (!path) return;

    try {
      await registerProject(path);
      setNewProjectPath('');
      setShowNewProject(false);
    } catch (err) {
      console.error('Failed to register project:', err);
    }
  };

  const session = () => currentSession();
  const project = () => currentProject();

  return (
    <div class="h-full flex flex-col bg-neutral-900 text-neutral-100">
      <div class="p-3 border-b border-neutral-800">
        <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">Projects</h2>
      </div>

      <div class="flex-1 overflow-y-auto">
        <div class="p-2">
          <For each={projects()}>
            {(p) => (
              <ProjectItem
                project={p}
                selected={project()?.path === p.path}
                onSelect={() => selectProject(p.path)}
              />
            )}
          </For>

          <Show when={projects().length === 0 && !showNewProject()}>
            <p class="text-neutral-500 text-sm text-center py-4">No projects registered</p>
          </Show>

          <Show when={showNewProject()}>
            <div class="mt-2 p-2 bg-neutral-800 rounded-lg">
              <input
                type="text"
                value={newProjectPath()}
                onInput={(e) => setNewProjectPath(e.currentTarget.value)}
                placeholder="/path/to/project"
                class="w-full bg-neutral-700 text-neutral-100 px-2 py-1 rounded text-sm"
              />
              <div class="flex gap-2 mt-2">
                <button
                  onClick={handleRegisterProject}
                  class="flex-1 px-2 py-1 bg-blue-600 text-white rounded text-sm hover:bg-blue-700"
                >
                  Add
                </button>
                <button
                  onClick={() => setShowNewProject(false)}
                  class="px-2 py-1 bg-neutral-700 text-neutral-300 rounded text-sm hover:bg-neutral-600"
                >
                  Cancel
                </button>
              </div>
            </div>
          </Show>

          <button
            onClick={() => setShowNewProject(true)}
            class="w-full mt-2 px-3 py-2 text-sm text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors flex items-center justify-center gap-2"
          >
            <Plus class="w-3.5 h-3.5" />
            Add Project
          </button>
        </div>

        <Show when={project()}>
          <div class="border-t border-neutral-800">
            <KilnSelector
              kilns={project()!.kilns}
              selected={selectedKiln()}
              onSelect={handleKilnSelect}
            />

            <div class="p-3 flex items-center justify-between">
              <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">Sessions</h2>
              <select
                data-testid="session-filter-dropdown"
                value={sessionFilter()}
                onChange={(e) => setSessionFilter(e.target.value as 'active' | 'all' | 'archived')}
                class="text-xs bg-neutral-800 text-neutral-300 border border-neutral-700 rounded px-1 py-0.5"
              >
                <option value="active">Active</option>
                <option value="all">All</option>
                <option value="archived">Archived</option>
              </select>
            </div>

            <div class="px-3 pb-2">
              <div class="relative">
                <Search class="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-neutral-500" />
                <input
                  ref={searchInputRef}
                  type="text"
                  value={searchQuery()}
                  onInput={(e) => handleSearchInput(e.currentTarget.value)}
                  placeholder="Search sessions..."
                  class="w-full bg-neutral-800 text-neutral-200 text-sm pl-8 pr-7 py-1.5 rounded border border-neutral-700 focus:border-primary focus:outline-none placeholder:text-neutral-500"
                  data-testid="session-search-input"
                />
                <Show when={searchQuery()}>
                  <button
                    onClick={clearSearch}
                    class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-neutral-500 hover:text-neutral-300 rounded"
                  >
                    <X class="w-3 h-3" />
                  </button>
                </Show>
              </div>
            </div>

            <div class="p-2" data-testid="session-list">
              <Show when={isSearching()}>
                <p class="text-neutral-500 text-sm text-center py-2">Searching...</p>
              </Show>

              {(() => {
                const hasProviders = createMemo(() => providers().length > 0);
                const isDisabled = createMemo(() => isLoading() || !selectedKiln() || !hasProviders());
                return (
                  <>
                    <button
                      onClick={handleCreateSession}
                      disabled={isDisabled()}
                      class="w-full mt-2 px-3 py-2 text-sm text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
                      data-testid="new-session-button"
                    >
                      <Plus class="w-3.5 h-3.5" />
                      New Session
                    </button>
                    <Show when={!hasProviders()}>
                      <p class="text-xs text-neutral-500 text-center mt-1">No LLM providers detected</p>
                    </Show>
                  </>
                );
              })()}

              <For each={displayedSessions()}>
                {(s) => (
                  <SessionItem
                    session={s}
                    selected={session()?.id === s.id}
                    onSelect={() => selectSession(s.id)}
                    onArchive={() => archiveSession(s.id)}
                    onDelete={() => deleteSession(s.id)}
                  />
                )}
              </For>

              <Show when={!isSearching() && displayedSessions().length === 0}>
                <Show
                  when={searchQuery().trim()}
                  fallback={<p class="text-neutral-500 text-sm text-center py-4">No sessions</p>}
                >
                  <p class="text-neutral-500 text-sm text-center py-4">No sessions match "{searchQuery()}"</p>
                </Show>
              </Show>

            </div>
          </div>
        </Show>
      </div>

      <Show when={session()}>
        <div class="border-t border-neutral-800 p-3">
          <div class="flex items-center gap-2 mb-2">
            <StateIndicator state={session()!.state} />
            <span class="text-sm font-medium">{session()!.state}</span>
          </div>

          <div class="flex gap-2">
            <Show when={session()!.state === 'active'}>
              <button
                onClick={pauseSession}
                class="flex-1 px-2 py-1 text-sm bg-yellow-600 text-white rounded hover:bg-yellow-700"
              >
                Pause
              </button>
            </Show>

            <Show when={session()!.state === 'paused'}>
              <button
                onClick={resumeSession}
                class="flex-1 px-2 py-1 text-sm bg-green-600 text-white rounded hover:bg-green-700"
              >
                Resume
              </button>
            </Show>


            <button
              onClick={() => refreshSessions()}
              class="px-2 py-1 text-sm bg-neutral-700 text-neutral-300 rounded hover:bg-neutral-600 flex items-center justify-center"
            >
              <RefreshCw class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};
