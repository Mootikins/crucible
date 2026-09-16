import { describe, it, expect } from 'vitest';
import { resolveSessionRoot, sessionRoots } from '../session-roots';
import type { KilnListEntry, Project, Session } from '@/lib/types';

const KILNS: KilnListEntry[] = [
  { path: '/home/me/docs', name: 'docs', last_access_secs_ago: null, open: true, registered: true },
  { path: '/home/me/notes', name: 'notes', last_access_secs_ago: null, open: true, registered: true },
  { path: '/home/me/archive', name: 'archive', last_access_secs_ago: null, open: true, registered: true },
];

const PROJECTS: Project[] = [
  {
    path: '/home/me/crucible',
    name: 'crucible',
    kilns: [],
    last_accessed: '2026-01-01T00:00:00Z',
  },
  {
    path: '/home/me/other-repo',
    name: 'other-repo',
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

  it('offers every unattached kiln and non-workspace project separately', () => {
    const { others } = sessionRoots(
      session({ workspace: '/home/me/crucible', kilns: ['docs'] }),
      KILNS,
      PROJECTS,
    );
    expect(others.map((r) => [r.name, r.origin])).toEqual([
      ['notes', 'other-kiln'],
      ['archive', 'other-kiln'],
      ['other-repo', 'other-project'],
    ]);
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
    expect(roots.others.map((r) => r.name)).toEqual([
      'docs',
      'notes',
      'archive',
      'crucible',
      'other-repo',
    ]);
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

  // Pinning a REGISTERED PROJECT that is not this session's workspace must
  // browse it, not silently fall back: the roster offers every project, and a
  // pick that does nothing reads as broken. Projects get the same
  // browse-not-attach treatment kilns already have.
  it('honours a pin to a registered project the session does not work in', () => {
    const pinned = resolveSessionRoot(roots(), 'project:/home/me/other-repo');
    expect(pinned?.name).toBe('other-repo');
    expect(pinned?.origin).toBe('other-project');
  });

  // An unregistered directory was never offered by the roster; a stale pin to
  // one must not invent a root out of thin air.
  it('still falls back for a pin to an unregistered project path', () => {
    expect(resolveSessionRoot(roots(), 'project:/home/me/gone')?.name).toBe('crucible');
  });

  it('is null when the session reaches nothing at all', () => {
    expect(resolveSessionRoot(sessionRoots(session(), [], []), null)).toBeNull();
  });
});
