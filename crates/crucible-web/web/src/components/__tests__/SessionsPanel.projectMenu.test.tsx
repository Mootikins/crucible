import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Project } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

let projectList: Project[] = [];

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => null,
    sessions: () => [],
    selectSession: async () => {},
    archiveSession: async () => {},
    deleteSession: async () => {},
    refreshSessions: async () => {},
  }),
}));

const selectProject = vi.fn(async (_path: string) => {});
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    projects: () => projectList,
    currentProject: () => projectList[0] ?? null,
    selectProject: (path: string) => selectProject(path),
  }),
}));

import { SessionsPanel } from '../SessionsPanel';

/**
 * The rail asks the workspace providers for each repository root through the
 * shared query. No project here has a repository, so nothing is asked — the
 * client and the mock fetch are installed to prove that, and to keep one
 * case's cache out of the next.
 */
let env: TestQueryEnv;

beforeEach(() => {
  env = createTestQueryEnv();
});

afterEach(() => {
  env.restore();
});


const project = (path: string, name: string): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '2026-01-01T00:00:00Z',
});

/**
 * The project actions moved off the rail's kebab (now the layout control) and
 * into the sessions pane, which is the surface they act on. They stay a
 * ROSTER-wide menu rather than rows on the project tier: the tier draws only
 * projects that already have a session, so a project you have not started
 * work in has no row to pin from.
 */
describe('SessionsPanel — the project control', () => {
  beforeEach(() => {
    localStorage.clear();
    selectProject.mockClear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
  });

  it('puts the project menu on the Projects section header', () => {
    const { getByTestId } = render(() => <SessionsPanel />);
    const row = getByTestId('projects-section').parentElement!;
    expect(row.querySelector('[data-testid="project-menu"]')).toBeTruthy();
  });

  it('lists every registered project, including one with no session', async () => {
    const { getByTestId } = render(() => <SessionsPanel />);
    fireEvent.click(getByTestId('project-menu'));
    const item = await waitFor(() => {
      const el = document.querySelector<HTMLElement>(
        '[data-testid="project-pin-/home/me/atlas"]',
      );
      expect(el).toBeTruthy();
      return el!;
    });
    fireEvent.pointerDown(item);
    fireEvent.click(item);
    await waitFor(() => expect(selectProject).toHaveBeenCalledWith('/home/me/atlas'));
  });

  it('offers a second window per project', async () => {
    const { getByTestId } = render(() => <SessionsPanel />);
    fireEvent.click(getByTestId('project-menu'));
    await waitFor(() =>
      expect(
        document.querySelector('[data-testid="project-new-window-/home/me/atlas"]'),
      ).toBeTruthy(),
    );
  });
});
