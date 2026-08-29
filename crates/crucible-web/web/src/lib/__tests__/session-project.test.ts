import { describe, it, expect } from 'vitest';
import { projectOfSession, sessionInProject } from '@/lib/session-project';
import type { Project } from '@/lib/types';

const project = (path: string, name: string): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '2026-01-01T00:00:00Z',
});

const CRUCIBLE = project('/home/me/crucible', 'crucible');
const NESTED = project('/home/me/crucible/vendor/markdown-it', 'markdown-it');
const ATLAS = project('/home/me/atlas', 'atlas');

describe('sessionInProject', () => {
  it('matches a workspace that IS the project directory', () => {
    expect(sessionInProject({ workspace: '/home/me/crucible' }, CRUCIBLE)).toBe(true);
  });

  it('matches a workspace under the project directory', () => {
    expect(sessionInProject({ workspace: '/home/me/crucible/crates' }, CRUCIBLE)).toBe(true);
  });

  it('ignores a trailing slash on either side', () => {
    expect(sessionInProject({ workspace: '/home/me/crucible/' }, CRUCIBLE)).toBe(true);
  });

  it('does not match a sibling whose path merely shares a prefix', () => {
    expect(sessionInProject({ workspace: '/home/me/crucible-old' }, CRUCIBLE)).toBe(false);
  });

  it('puts a workspace-less session in NO project rather than the pinned one', () => {
    expect(sessionInProject({ workspace: null }, CRUCIBLE)).toBe(false);
    expect(sessionInProject({ workspace: '' }, CRUCIBLE)).toBe(false);
  });
});

describe('projectOfSession', () => {
  it('gives the deepest registered project, not the first that contains it', () => {
    const found = projectOfSession(
      { workspace: '/home/me/crucible/vendor/markdown-it/src' },
      [CRUCIBLE, NESTED, ATLAS],
    );
    expect(found?.name).toBe('markdown-it');
  });

  it('answers null when no project contains the workspace', () => {
    expect(projectOfSession({ workspace: '/tmp/scratch' }, [CRUCIBLE, ATLAS])).toBeNull();
  });
});
