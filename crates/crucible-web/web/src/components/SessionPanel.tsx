import { Component, Show, createSignal, createEffect } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { useSessionSearch } from '@/hooks/useSessionSearch';
import { ProjectSection } from './ProjectSection';
import { SessionSection } from './SessionSection';
import { SessionFooter } from './SessionFooter';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';

export const SessionPanel: Component = () => {
  const { currentProject, projects, selectProject, registerProject } = useProjectSafe();
  const {
    currentSession,
    sessions,
    isLoading,
    selectSession,
    pauseSession,
    resumeSession,
    refreshSessions,
    providers,
    deleteSession,
    archiveSession,
  } = useSessionSafe();

  const [selectedKiln, setSelectedKiln] = createSignal<string>('');
  const [sessionFilter, setSessionFilter] = createSignal<'active' | 'all' | 'archived'>('active');

  const search = useSessionSearch({
    selectedKiln,
    sessions,
    sessionFilter,
  });

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

  // All entry points converge on the draft surface (lazy creation) — no
  // hardcoded provider/model fallbacks here anymore.
  const handleCreateSession = () => {
    window.dispatchEvent(new CustomEvent('crucible:new-session'));
  };

  const handleKilnSelect = async (kiln: string) => {
    setSelectedKiln(kiln);
    await refreshSessions({ kiln, workspace: currentProject()?.path });
  };

  const session = () => currentSession();
  const project = () => currentProject();

  return (
    <PanelShell>
      <PanelHeader title="Projects" />

      <div class="flex-1 overflow-y-auto">

        <ProjectSection
          projects={projects()}
          currentProject={project() ?? undefined}
          onSelectProject={selectProject}
          onRegisterProject={async (path) => { await registerProject(path); }}
        />

        <Show when={project()}>
          <SessionSection
            kilns={project()!.kilns}
            selectedKiln={selectedKiln()}
            onKilnSelect={handleKilnSelect}
            sessionFilter={sessionFilter()}
            onSessionFilterChange={setSessionFilter}
            searchQuery={search.searchQuery()}
            isSearching={search.isSearching()}
            onSearchInput={search.handleSearchInput}
            onClearSearch={search.clearSearch}
            setSearchInputRef={search.setSearchInputRef}
            displayedSessions={search.displayedSessions()}
            currentSession={session() ?? undefined}
            onSelectSession={selectSession}
            onArchiveSession={archiveSession}
            onDeleteSession={deleteSession}
            onCreateSession={handleCreateSession}
            isLoading={isLoading()}
            hasProviders={providers().length > 0}
          />
        </Show>
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
