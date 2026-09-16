import {
  createContext,
  useContext,
  ParentComponent,
  createEffect,
  createSignal,
} from 'solid-js';
import type { Project } from '@/lib/types';
import type { ProjectContextValue } from '@/lib/types/context';
import { projectFromUrl } from '@/lib/project-url';
import {
  fetchProjectOnce,
  useProjects,
  useRegisterProject,
  useUnregisterProject,
} from '@/lib/query/projects';


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

/**
 * Which project the shell is pointed at.
 *
 * The roster itself is NOT held here any more: `useProjects()` owns it, so the
 * composer, the files pane and this context read one list from one fetch, and
 * a project registered anywhere appears everywhere. What stays is the part
 * that is this browser's alone — the selection and the pin that survives a
 * reload. Display state belongs to the client; the roster belongs to the
 * daemon.
 */
export const ProjectProvider: ParentComponent = (props) => {
  const [currentProject, setCurrentProject] = createSignal<Project | null>(null);
  const projectsQuery = useProjects();
  const projects = () => projectsQuery.data ?? [];
  const register = useRegisterProject();
  const unregister = useUnregisterProject();
  // The failure of one action, which is not the failure of the roster read.
  const [actionError, setActionError] = createSignal<string | null>(null);

  const error = () =>
    actionError() ?? (projectsQuery.error ? projectsQuery.error.message : null);
  const isLoading = () =>
    projectsQuery.isFetching || register.isPending || unregister.isPending;

  const refreshProjects = async () => {
    await projectsQuery.refetch();
  };

  const registerProject = async (path: string): Promise<Project> => {
    setActionError(null);
    try {
      const project = await register.mutateAsync(path);
      setCurrentProject(project);
      return project;
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to register project');
      throw err;
    }
  };

  const unregisterProject = async (path: string) => {
    setActionError(null);
    try {
      await unregister.mutateAsync(path);
      if (currentProject()?.path === path) setCurrentProject(null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to unregister project';
      setActionError(msg);
      console.error('Failed to unregister project:', err);
    }
  };

  const selectProject = async (path: string) => {
    const existing = projects().find((p) => p.path === path);
    if (existing) {
      setCurrentProject(existing);
      rememberPin(path);
      return;
    }

    setActionError(null);
    try {
      const project = await fetchProjectOnce(path);
      if (project) {
        setCurrentProject(project);
        rememberPin(path);
      } else {
        setActionError(`Project not found: ${path}`);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to load project';
      setActionError(msg);
      console.error('Failed to select project:', err);
    }
  };

  const clearProject = () => {
    setCurrentProject(null);
  };

  // Cold start: pin one project, once, and only against a roster the daemon
  // answered. `dataUpdatedAt` is 0 while the seeded last-known roster is on
  // screen, so a pin cannot be spent on a project that was unregistered since
  // the previous run. A refused read ends the wait too: an offline reload
  // pins from the last-known roster rather than pinning nothing at all.
  let pinned = false;
  createEffect(() => {
    if (pinned || currentProject()) return;
    if (projectsQuery.dataUpdatedAt === 0 && !projectsQuery.isError) return;
    const list = projects();
    if (list.length === 0) return;
    pinned = true;
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
      ? list.find((p) => trimSlash(p.path) === trimSlash(remembered))
      : undefined;
    setCurrentProject(match ?? list[0]);
  });

  const value: ProjectContextValue = {
    currentProject,
    projects,
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
