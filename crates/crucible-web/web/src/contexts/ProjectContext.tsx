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
import { projectFromUrl } from '@/lib/project-url';
import {
  registerProject as apiRegisterProject,
  unregisterProject as apiUnregisterProject,
  listProjects as apiListProjects,
  getProject as apiGetProject,
} from '@/lib/api';


const ProjectContext = createContext<ProjectContextValue>();

const PIN_KEY = 'crucible:pinnedProject';

function trimSlash(p: string): string {
  return p.replace(/\/+$/, '');
}

/** The project this browser last pinned, or null. */
function cachedPinPath(): string | null {
  try {
    return localStorage.getItem(PIN_KEY) || null;
  } catch {
    return null;
  }
}

function rememberPin(path: string): void {
  try {
    localStorage.setItem(PIN_KEY, path);
  } catch {
    /* private mode */
  }
}

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
      rememberPin(path);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const project = await apiGetProject(path);
      if (project) {
        setCurrentProject(project);
        rememberPin(path);
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
    if (currentProject() || projects.length === 0) return;
    // A window opened from "Open <project> in a new window" is ADDRESSED to
    // that project. Without this it ran the same cold-start rule as the first
    // window and pinned projects[0], so every row but the first opened the
    // wrong project under the right label.
    // Order: the URL a "new window" was addressed to, then the pin this
    // browser last held, then first-by-roster. The middle step exists because
    // scope is supposed to change only on an explicit action, and without it
    // F5 silently repointed the file tree and the switcher's Recent list.
    const remembered = projectFromUrl() ?? cachedPinPath();
    const match = remembered
      ? projects.find((p) => trimSlash(p.path) === trimSlash(remembered))
      : undefined;
    setCurrentProject(match ?? projects[0]);
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
