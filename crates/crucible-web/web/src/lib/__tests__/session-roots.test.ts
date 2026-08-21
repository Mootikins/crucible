import { describe, it, expect } from 'vitest';
import { resolveSessionRoot, sessionRoots } from '../session-roots';
import type { KilnListEntry, Project, Session } from '@/lib/types';

const KILNS: KilnListEntry[] = [
  { path: '/home/me/docs', name: 'docs' },
  { path: '/home/me/notes', name: 'notes' },
  { path: '/home/me/archive', name: 'archive' },
];

const PROJECTS: Project[] = [
  {
    path: '/home/me/crucible',
    name: 'crucible',
    kilns: [],
    last_accessed: '2026-01-01T00:00:00Z',
  },
];

const session = (over: Partial<Session> = {}): Session =>
  ({
    id: 's-1',
    session_type: 'chat',
    kilns: [],
    workspace: null,
    state: 'idle',
    title: null,
    agent_model: null,
    agent_mode: null,
    started_at: '2026-01-01T00:00:00Z',
    event_count: 0,
    ...over,
  }) as Session;

describe('sessionRoots', () => {
  it('puts the workspace first, then the attached kilns', () => {
    const { own } = sessionRoots(
      session({ workspace: '/home/me/crucible', kilns: ['docs', 'notes'] }),
      KILNS,
      PROJECTS,
    );
    expect(own.map((r) => [r.name, r.origin])).toEqual([
      ['crucible', 'workspace'],
      ['docs', 'attached-kiln'],
      ['notes', 'attached-kiln'],
    ]);
  });

  it('offers every unattached kiln separately', () => {
    const { others } = sessionRoots(
      session({ workspace: '/home/me/crucible', kilns: ['docs'] }),
      KILNS,
      PROJECTS,
    );
    expect(others.map((r) => r.name)).toEqual(['notes', 'archive']);
    expect(others.every((r) => r.origin === 'other-kiln')).toBe(true);
  });

  // A session with no workspace is a legitimate shape (a tools-only agent),
  // not a degenerate one.
  it('handles a session with no workspace', () => {
    const { own } = sessionRoots(session({ kilns: ['docs'] }), KILNS, PROJECTS);
    expect(own.map((r) => r.name)).toEqual(['docs']);
  });

  it('has no roots at all with no session', () => {
    const roots = sessionRoots(null, KILNS, PROJECTS);
    expect(roots.own).toEqual([]);
    expect(roots.others.map((r) => r.name)).toEqual(['docs', 'notes', 'archive']);
  });

  // `kilnPathForName` answers null for an unknown name, and null is not a
  // root: coercing it to '' points the tree at the daemon data dir, a far
  // wider corpus than the one the session attached.
  it('drops an attached kiln the registry cannot resolve rather than rooting at nothing', () => {
    const { own } = sessionRoots(session({ kilns: ['docs', 'ghost'] }), KILNS, PROJECTS);
    expect(own.map((r) => r.name)).toEqual(['docs']);
  });

  it('names a workspace after its registered project, not its basename', () => {
    const { own } = sessionRoots(session({ workspace: '/home/me/crucible/' }), KILNS, [
      { ...PROJECTS[0], name: 'Crucible (main)' },
    ]);
    expect(own[0].name).toBe('Crucible (main)');
  });

  it('falls back to the basename for an unregistered workspace', () => {
    const { own } = sessionRoots(session({ workspace: '/tmp/scratch-dir' }), KILNS, PROJECTS);
    expect(own[0].name).toBe('scratch-dir');
  });
});

describe('resolveSessionRoot', () => {
  const roots = () =>
    sessionRoots(
      session({ workspace: '/home/me/crucible', kilns: ['docs'] }),
      KILNS,
      PROJECTS,
    );

  it('follows the session when nothing is pinned', () => {
    expect(resolveSessionRoot(roots(), null)?.name).toBe('crucible');
  });

  it('honours a pin to one of the session’s own roots', () => {
    expect(resolveSessionRoot(roots(), 'kiln:/home/me/docs')?.name).toBe('docs');
  });

  // Pinning is how you browse a corpus the session cannot query; that has to
  // survive a switch away and back, so the pin is checked against BOTH lists.
  it('honours a pin to a kiln the session is only browsing', () => {
    const pinned = resolveSessionRoot(roots(), 'kiln:/home/me/archive');
    expect(pinned?.name).toBe('archive');
    expect(pinned?.origin).toBe('other-kiln');
  });

  it('falls back when the pin no longer resolves', () => {
    expect(resolveSessionRoot(roots(), 'kiln:/home/me/deleted')?.name).toBe('crucible');
  });

  it('is null when the session reaches nothing at all', () => {
    expect(resolveSessionRoot(sessionRoots(session(), [], []), null)).toBeNull();
  });
});
