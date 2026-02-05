import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  onMount,
  Accessor,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { Project } from '@/lib/types';
import {
  registerProject as apiRegisterProject,
  unregisterProject as apiUnregisterProject,
  listProjects as apiListProjects,
  getProject as apiGetProject,
} from '@/lib/api';

export interface ProjectContextValue {
  currentProject: Accessor<Project | null>;
  projects: Accessor<Project[]>;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
  registerProject: (path: string) => Promise<Project>;
  unregisterProject: (path: string) => Promise<void>;
  selectProject: (path: string) => Promise<void>;
  refreshProjects: () => Promise<void>;
  clearProject: () => void;
}

const ProjectContext = createContext<ProjectContextValue>();

export const ProjectProvider: ParentComponent = (props) => {
  const [currentProject, setCurrentProject] = createSignal<Project | null>(null);
  const [projects, setProjects] = createStore<Project[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const refreshProjects = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const list = await apiListProjects();
      setProjects(list);
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

  onMount(() => {
    refreshProjects();
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

export function useProject(): ProjectContextValue {
  const context = useContext(ProjectContext);
  if (!context) {
    throw new Error('useProject must be used within a ProjectProvider');
  }
  return context;
}
