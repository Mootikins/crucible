import { Component, For, Show, createSignal, createEffect } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useProjectSafe } from '@/contexts/ProjectContext';
import type { Session, Project } from '@/lib/types';

const KilnSelector: Component<{
  kilns: string[];
  selected: string;
  onSelect: (kiln: string) => void;
}> = (props) => {
  const getKilnName = (path: string) => {
    const parts = path.split('/');
    return parts[parts.length - 1] || path;
  };

  return (
    <Show when={props.kilns.length > 0}>
      <div class="px-3 py-2 border-b border-neutral-800">
        <label class="text-xs text-neutral-500 block mb-1">Kiln</label>
        <Show
          when={props.kilns.length > 1}
          fallback={
            <div class="text-sm text-neutral-300 truncate" title={props.kilns[0]}>
              {getKilnName(props.kilns[0])}
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
                <option value={kiln}>{getKilnName(kiln)}</option>
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

const SessionItem: Component<{ session: Session; selected: boolean; onSelect: () => void }> = (props) => {
   return (
     <button
       onClick={props.onSelect}
       class={`w-full text-left px-3 py-2 rounded-lg transition-colors ${
         props.selected
           ? 'bg-primary/20 text-primary-hover'
           : 'hover:bg-surface-elevated text-neutral-300'
       }`}
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
    </button>
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
    endSession,
    refreshSessions,
    selectedProvider,
  } = useSessionSafe();

  const [showNewProject, setShowNewProject] = createSignal(false);
  const [newProjectPath, setNewProjectPath] = createSignal('');
  const [selectedKiln, setSelectedKiln] = createSignal<string>('');

  createEffect(() => {
    const project = currentProject();
    if (project && project.kilns.length > 0) {
      if (!selectedKiln() || !project.kilns.includes(selectedKiln())) {
        setSelectedKiln(project.kilns[0]);
      }
    }
  });

  const handleCreateSession = async () => {
    const project = currentProject();
    const kiln = selectedKiln();
    if (!project || !kiln) return;

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
            class="w-full mt-2 px-3 py-2 text-sm text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors"
          >
            + Add Project
          </button>
        </div>

        <Show when={project()}>
          <div class="border-t border-neutral-800">
            <KilnSelector
              kilns={project()!.kilns}
              selected={selectedKiln()}
              onSelect={handleKilnSelect}
            />

            <div class="p-3">
              <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">Sessions</h2>
            </div>

            <div class="p-2">
              <For each={sessions()}>
                {(s) => (
                  <SessionItem
                    session={s}
                    selected={session()?.id === s.id}
                    onSelect={() => selectSession(s.id)}
                  />
                )}
              </For>

              <Show when={sessions().length === 0}>
                <p class="text-neutral-500 text-sm text-center py-4">No sessions</p>
              </Show>

              <button
                onClick={handleCreateSession}
                disabled={isLoading() || !selectedKiln()}
                class="w-full mt-2 px-3 py-2 text-sm text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800 rounded-lg transition-colors disabled:opacity-50"
              >
                + New Session
              </button>
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

            <Show when={session()!.state !== 'ended'}>
              <button
                onClick={endSession}
                class="flex-1 px-2 py-1 text-sm bg-red-600 text-white rounded hover:bg-red-700"
              >
                End
              </button>
            </Show>

            <button
              onClick={() => refreshSessions()}
              class="px-2 py-1 text-sm bg-neutral-700 text-neutral-300 rounded hover:bg-neutral-600"
            >
              ↻
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};
