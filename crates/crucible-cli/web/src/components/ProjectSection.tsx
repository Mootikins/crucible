import { Component, For, Show, createSignal } from 'solid-js';
import type { Project } from '@/lib/types';
import { Plus } from '@/lib/icons';

const ProjectItem: Component<{ project: Project; selected: boolean; onSelect: () => void }> = (props) => {
   return (
     <button
       onClick={props.onSelect}
       class={`w-full text-left px-3 py-2 rounded-lg transition-colors ${
         props.selected
           ? 'bg-primary/20 text-primary-hover'
           : 'hover:bg-surface-elevated text-shell-body'
       }`}
     >
      <div class="font-medium truncate">{props.project.name}</div>
      <div class="text-xs text-muted-dark truncate">{props.project.path}</div>
    </button>
  );
};

export const ProjectSection: Component<{
  projects: Project[];
  currentProject: Project | undefined;
  onSelectProject: (path: string) => void;
  onRegisterProject: (path: string) => Promise<void>;
}> = (props) => {
  const [showNewProject, setShowNewProject] = createSignal(false);
  const [newProjectPath, setNewProjectPath] = createSignal('');

  const handleRegisterProject = async () => {
    const path = newProjectPath().trim();
    if (!path) return;

    try {
      await props.onRegisterProject(path);
      setNewProjectPath('');
      setShowNewProject(false);
    } catch (err) {
      console.error('Failed to register project:', err);
    }
  };

  return (
    <div class="p-2">
      <For each={props.projects}>
        {(p) => (
          <ProjectItem
            project={p}
            selected={props.currentProject?.path === p.path}
            onSelect={() => props.onSelectProject(p.path)}
          />
        )}
      </For>

      <Show when={props.projects.length === 0 && !showNewProject()}>
        <p class="text-muted-dark text-sm text-center py-4">No projects registered</p>
      </Show>

      <Show when={showNewProject()}>
        <div class="mt-2 p-2 bg-surface-elevated rounded-lg">
          <input
            type="text"
            value={newProjectPath()}
            onInput={(e) => setNewProjectPath(e.currentTarget.value)}
            placeholder="/path/to/project"
            class="w-full bg-control text-shell-ink px-2 py-1 rounded text-sm"
          />
          <div class="flex gap-2 mt-2">
            <button
              onClick={handleRegisterProject}
              class="flex-1 px-2 py-1 bg-primary text-white rounded text-sm hover:bg-primary-hover"
            >
              Add
            </button>
            <button
              onClick={() => setShowNewProject(false)}
              class="px-2 py-1 bg-control text-shell-body rounded text-sm hover:bg-hover-wash"
            >
              Cancel
            </button>
          </div>
        </div>
      </Show>

      <button
        onClick={() => setShowNewProject(true)}
        class="w-full mt-2 px-3 py-2 text-sm text-muted hover:text-shell-ink hover:bg-hover-wash rounded-lg transition-colors flex items-center justify-center gap-2"
      >
        <Plus class="w-3.5 h-3.5" />
        Add Project
      </button>
    </div>
  );
};
