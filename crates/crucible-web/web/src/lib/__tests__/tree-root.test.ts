import { describe, it, expect } from 'vitest';
import { buildRoster, rosterIndex, rootKey, type KilnListEntry } from '@/lib/tree-root';
import type { Project } from '@/lib/types';

const project = (path: string, name: string, kilns: Project['kilns'] = []): Project => ({
  path,
  name,
  kilns,
  last_accessed: '',
});
const kiln = (path: string, name: string | null = null): KilnListEntry => ({ path, name });

describe('buildRoster', () => {
  it('returns two empty groups for empty inputs, labelled Projects then Kilns', () => {
    const groups = buildRoster([], []);
    expect(groups.map((g) => g.label)).toEqual(['Projects', 'Kilns']);
    expect(groups[0].roots).toEqual([]);
    expect(groups[1].roots).toEqual([]);
  });

  it('falls back to basename when a project name is empty', () => {
    const groups = buildRoster([project('/home/me/code/app', '')], []);
    expect(groups[0].roots[0]).toMatchObject({ kind: 'project', path: '/home/me/code/app', name: 'app' });
  });

  it('uses kiln names, falling back to basename on null', () => {
    const groups = buildRoster([], [kiln('/vault', 'My Vault'), kiln('/other/docs', null)]);
    const kilnRoots = groups[1].roots;
    expect(kilnRoots).toEqual([
      { kind: 'kiln', path: '/vault', name: 'My Vault' },
      { kind: 'kiln', path: '/other/docs', name: 'docs' },
    ]);
  });

  it('normalizes a .crucible config dir to the kiln root', () => {
    const groups = buildRoster([], [kiln('/vault/.crucible', 'default')]);
    expect(groups[1].roots[0]).toMatchObject({ kind: 'kiln', path: '/vault', name: 'default' });
  });

  it('preserves first-seen order for kilns', () => {
    const groups = buildRoster([], [kiln('/b'), kiln('/a'), kiln('/c')]);
    expect(groups[1].roots.map((r) => r.path)).toEqual(['/b', '/a', '/c']);
  });

  it('dedupes a kiln appearing in both /api/kilns and a project attachment', () => {
    const groups = buildRoster(
      [project('/proj', 'Proj', [{ path: '/vault', name: 'shared' }])],
      [kiln('/vault', 'shared')],
    );
    expect(groups[1].roots.filter((r) => r.path === '/vault')).toHaveLength(1);
  });

  it('dedupes the same kiln attached to two projects', () => {
    const groups = buildRoster(
      [
        project('/p1', 'P1', [{ path: '/vault', name: 'v' }]),
        project('/p2', 'P2', [{ path: '/vault', name: 'v' }]),
      ],
      [],
    );
    expect(groups[1].roots.filter((r) => r.path === '/vault')).toHaveLength(1);
  });

  it('does NOT collapse a project and a same-path kiln (rootKey includes kind)', () => {
    const groups = buildRoster([project('/vault', 'V')], [kiln('/vault', 'V')]);
    expect(groups[0].roots[0].path).toBe('/vault');
    expect(groups[1].roots[0].path).toBe('/vault');
    expect(rootKey(groups[0].roots[0])).toBe('project:/vault');
    expect(rootKey(groups[1].roots[0])).toBe('kiln:/vault');
    expect(rootKey(groups[0].roots[0])).not.toBe(rootKey(groups[1].roots[0]));
  });
});

describe('rosterIndex', () => {
  it('maps every rootKey to its TreeRoot', () => {
    const groups = buildRoster([project('/p', 'P')], [kiln('/k', 'K')]);
    const idx = rosterIndex(groups);
    expect(idx.get('project:/p')).toMatchObject({ kind: 'project', path: '/p' });
    expect(idx.get('kiln:/k')).toMatchObject({ kind: 'kiln', path: '/k' });
    expect(idx.size).toBe(2);
  });
});
