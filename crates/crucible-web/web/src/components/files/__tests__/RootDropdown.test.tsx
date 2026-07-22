import { describe, it, expect, vi } from 'vitest';
import { createMemo, createSignal } from 'solid-js';
import { render, fireEvent } from '@solidjs/testing-library';
import { RootDropdown } from '../RootDropdown';
import { buildRoster, rootKey, rosterIndex, type TreeRoot } from '@/lib/tree-root';
import type { KilnListEntry, Project } from '@/lib/types';

const project = (path: string, name: string, kilns: Project['kilns'] = []): Project => ({
  path,
  name,
  kilns,
  last_accessed: '',
});

describe('RootDropdown', () => {
  it('renders grouped optgroups with the expected option counts', () => {
    const groups = buildRoster(
      [project('/p1', 'P1'), project('/p2', 'P2')],
      [{ path: '/vault', name: 'Vault' }],
    );
    const { container } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    const optgroups = container.querySelectorAll('optgroup');
    expect(optgroups).toHaveLength(2);
    expect(optgroups[0].label).toBe('Projects');
    expect(optgroups[0].querySelectorAll('option')).toHaveLength(2);
    expect(optgroups[1].label).toBe('Kilns');
    expect(optgroups[1].querySelectorAll('option')).toHaveLength(1);
  });

  // Regression: the FilesPanel wiring — projects and kilns land in separate
  // async flushes, the resolved root (persisted kiln) only becomes valid once
  // kilns arrive, and each roster rebuild re-creates every <option>. The DOM
  // select must END on the resolved key, not silently sit on option 0.
  it('DOM selection follows the resolved root across async roster arrival', async () => {
    const [projects, setProjects] = createSignal<Project[]>([]);
    const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
    const persisted = 'kiln:/vault';
    const roster = createMemo(() => buildRoster(projects(), kilns()));
    const activeKey = createMemo(() => {
      const idx = rosterIndex(roster());
      if (idx.has(persisted)) return persisted;
      const first = roster().find((g) => g.roots.length > 0)?.roots[0];
      return first ? rootKey(first) : null;
    });
    const { getByTestId } = render(() => (
      <RootDropdown groups={roster()} selectedKey={activeKey()} onSelect={() => {}} />
    ));

    setProjects([project('/p1', 'crucible')]);
    await Promise.resolve();
    expect((getByTestId('root-dropdown') as HTMLSelectElement).value).toBe('project:/p1');

    setKilns([{ path: '/vault', name: 'docs' }]);
    await Promise.resolve();
    expect((getByTestId('root-dropdown') as HTMLSelectElement).value).toBe('kiln:/vault');

    // A later roster rebuild (projects refresh) re-creates all options with
    // the SAME resolved key — the DOM selection must survive that too.
    setProjects([project('/p1', 'crucible'), project('/p2', 'other')]);
    await Promise.resolve();
    expect((getByTestId('root-dropdown') as HTMLSelectElement).value).toBe('kiln:/vault');
  });

  it('reflects the selected key on the <select>', () => {
    const groups = buildRoster([project('/p1', 'P1')], []);
    const { getByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey="project:/p1" onSelect={() => {}} />
    ));
    expect((getByTestId('root-dropdown') as HTMLSelectElement).value).toBe('project:/p1');
  });

  it('calls onSelect with the resolved TreeRoot when changed', () => {
    const groups = buildRoster([project('/p1', 'P1')], [{ path: '/vault', name: 'Vault' }]);
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const { getByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={onSelect} />
    ));
    const select = getByTestId('root-dropdown') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: 'kiln:/vault' } });
    expect(onSelect).toHaveBeenCalledWith({ kind: 'kiln', path: '/vault', name: 'Vault' });
  });

  it('omits empty groups (only non-empty ones render)', () => {
    const groups = buildRoster([], [{ path: '/vault', name: 'Vault' }]);
    const { container } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    const optgroups = container.querySelectorAll('optgroup');
    expect(optgroups).toHaveLength(1);
    expect(optgroups[0].label).toBe('Kilns');
  });

  it('shows a "No roots" fallback and no <select> for an empty roster', () => {
    const groups = buildRoster([], []);
    const { container, queryByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    expect(queryByTestId('root-dropdown')).toBeNull();
    expect(container.textContent).toContain('No roots');
  });
});
