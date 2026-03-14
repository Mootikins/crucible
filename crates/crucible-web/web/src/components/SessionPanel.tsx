import { Component, Show, createSignal, createEffect } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import { notificationActions } from '@/stores/notificationStore';
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
    createSession,
    selectSession,
    pauseSession,
    resumeSession,
    refreshSessions,
    selectedProvider,
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

  const session = () => currentSession();
  const project = () => currentProject();

  return (
    <PanelShell>
      <PanelHeader title="Projects" />

      <div class="flex-1 overflow-y-auto">

        <ProjectSection
          projects={projects()}
          currentProject={project()}
          onSelectProject={selectProject}
          onRegisterProject={registerProject}
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
            currentSession={session()}
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
