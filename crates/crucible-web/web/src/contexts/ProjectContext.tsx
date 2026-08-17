import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  onMount,
} from 'solid-js';
import { createStore, produce, reconcile } from 'solid-js/store';
import type { Project } from '@/lib/types';
import type { ProjectContextValue } from '@/lib/types/context';
import {
  registerProject as apiRegisterProject,
  unregisterProject as apiUnregisterProject,
  listProjects as apiListProjects,
  getProject as apiGetProject,
} from '@/lib/api';


const ProjectContext = createContext<ProjectContextValue>();

function cachedProjects(): Project[] {
  try {
    const raw = localStorage.getItem('crucible:cache:projects');
    return raw ? (JSON.parse(raw) as Project[]) : [];
  } catch {
    return [];
  }
}

export const ProjectProvider: ParentComponent = (props) => {
  const [currentProject, setCurrentProject] = createSignal<Project | null>(null);
  // Seed with the last-known roster so the shell paints instantly on reload.
  const [projects, setProjects] = createStore<Project[]>(cachedProjects());
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const refreshProjects = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const list = await apiListProjects();
      setProjects(reconcile(list));
      try {
        localStorage.setItem('crucible:cache:projects', JSON.stringify(list));
      } catch {
        /* private mode */
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load projects';
      setError(msg);
      console.error('Failed to refresh projects:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const registerProject = async (path: string): Promise<Project> => {
    setIsLoading(true);
    setError(null);

    try {
      const project = await apiRegisterProject(path);
      setProjects(produce((list) => list.unshift(project)));
      setCurrentProject(project);
      return project;
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to register project';
      setError(msg);
      throw err;
    } finally {
      setIsLoading(false);
    }
  };

  const unregisterProject = async (path: string) => {
    setIsLoading(true);
    setError(null);

    try {
      await apiUnregisterProject(path);
      setProjects(produce((list) => {
        const idx = list.findIndex((p) => p.path === path);
        if (idx !== -1) list.splice(idx, 1);
      }));

      if (currentProject()?.path === path) {
        setCurrentProject(null);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to unregister project';
      setError(msg);
      console.error('Failed to unregister project:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const selectProject = async (path: string) => {
    const existing = projects.find((p) => p.path === path);
    if (existing) {
      setCurrentProject(existing);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const project = await apiGetProject(path);
      if (project) {
        setCurrentProject(project);
      } else {
        setError(`Project not found: ${path}`);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load project';
      setError(msg);
      console.error('Failed to select project:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const clearProject = () => {
    setCurrentProject(null);
  };

  onMount(async () => {
    await refreshProjects();
    if (projects.length > 0 && !currentProject()) {
      setCurrentProject(projects[0]);
    }
  });

  const value: ProjectContextValue = {
    currentProject,
    projects: () => projects,
    isLoading,
    error,
    registerProject,
    unregisterProject,
    selectProject,
    refreshProjects,
    clearProject,
  };

  return (
    <ProjectContext.Provider value={value}>
      {props.children}
    </ProjectContext.Provider>
  );
};

const noopAsync = async () => {};
const noopPromise = <T,>() => Promise.resolve(undefined as unknown as T);

const fallbackProjectContext: ProjectContextValue = {
  currentProject: () => null,
  projects: () => [],
  isLoading: () => false,
  error: () => null,
  refreshProjects: noopAsync,
  selectProject: noopAsync,
  registerProject: noopPromise,
  unregisterProject: noopAsync,
  clearProject: () => {},
};

export function useProjectSafe(): ProjectContextValue {
  const context = useContext(ProjectContext);
  return context ?? fallbackProjectContext;
}
