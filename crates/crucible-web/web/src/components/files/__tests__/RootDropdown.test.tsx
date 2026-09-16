import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMemo, createSignal } from 'solid-js';
import { render, fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { RootDropdown } from '../RootDropdown';
import { buildRoster, rootKey, rosterIndex, type TreeRoot } from '@/lib/tree-root';
import type { SessionRoot } from '@/lib/session-roots';
import type { KilnListEntry, Project } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

/**
 * The daemon answers here, not `@/lib/api`.
 *
 * The dropdown reads its branch list and materialises a target through the
 * shared target queries, so stubbing the api module would prove only that the
 * stub was called. These routes are the two the workspace provider actually
 * goes through: the publication that declares it, and the plugin command it
 * declares.
 */
let env: TestQueryEnv;

/** One plugin command the daemon was asked to run. */
interface CommandCall {
  name: string;
  args: { workspace?: string; target?: string };
}

/** The rows `worktree:list` answers, as the plugin publishes them. */
let branchRows: Array<Record<string, unknown>> = [];
/** What `worktree:add` answers, or an error status when the provider refuses. */
let resolveAnswer: { path?: string | null } | { status: number } = {};
/** The paths `POST /api/project/register` was given, in order. */
let registered: string[] = [];
/** What that registration answers. */
let registerAnswer: Project = { path: '/registered', name: 'registered', kilns: [], last_accessed: '' };
let commands: CommandCall[] = [];

const ranCommand = (name: string) => commands.filter((c) => c.name === name);

const PUBLICATIONS = {
  publications: {
    targets: {
      worktree: {
        axis: 'workspace',
        label: 'Worktree',
        targets_command: 'worktree:list',
        resolve_command: 'worktree:add',
      },
    },
  },
};

function installDaemon(): void {
  env = createTestQueryEnv({
    'GET /api/plugins/publications': () => PUBLICATIONS,
    'POST /api/plugins/command': async (request) => {
      const body = (await request.json()) as CommandCall;
      commands.push(body);
      if (body.name === 'worktree:list') return { targets: branchRows };
      if ('status' in resolveAnswer) {
        return new Response(JSON.stringify({ error: 'the destination is busy' }), {
          status: resolveAnswer.status,
        });
      }
      return resolveAnswer;
    },
    'POST /api/project/register': async (request) => {
      registered.push(((await request.json()) as { path: string }).path);
      return registerAnswer;
    },
  });
}

const project = (path: string, name: string, kilns: Project['kilns'] = []): Project => ({
  path,
  name,
  kilns,
  last_accessed: '',
});

const openPopout = (getByTestId: (id: string) => HTMLElement) => {
  fireEvent.click(getByTestId('root-dropdown'));
};

const WORKSPACE: SessionRoot = {
  kind: 'project',
  path: '/home/me/crucible',
  name: 'crucible',
  origin: 'workspace',
};
const ATTACHED: SessionRoot = {
  kind: 'kiln',
  path: '/vault',
  name: 'Vault',
  origin: 'attached-kiln',
};

/** Section header + row labels in document order, so grouping is assertable. */
const rowLabels = (popout: HTMLElement): string[] =>
  [...popout.querySelectorAll('[role="option"]')].map((el) =>
    (el.querySelector('span')?.textContent ?? '').trim(),
  );

beforeEach(() => {
  // A repo-less root answers with no targets rather than throwing — the
  // enumerating calls swallow provider failure so one bad plugin cannot take
  // the picker down.
  branchRows = [];
  resolveAnswer = {};
  registerAnswer = project('/registered', 'registered');
  registered = [];
  commands = [];
  installDaemon();
});

afterEach(() => {
  env.restore();
});

describe('RootDropdown', () => {
  it('renders grouped section headers with the expected option counts', () => {
    const groups = buildRoster(
      [project('/p1', 'P1'), project('/p2', 'P2')],
      [{ path: '/vault', name: 'Vault' }],
    );
    const { getByTestId } = render(() => (
      <RootDropdown own={[]} groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    openPopout(getByTestId);
    const popout = screen.getByTestId('root-dropdown-popout');
    const rows = popout.querySelectorAll('[role="option"]');
    expect(rows).toHaveLength(3);
    expect(popout.textContent).toContain('Projects');
    expect(popout.textContent).toContain('Kilns');
    expect(popout.textContent).not.toContain('Worktrees'); // empty group omitted
  });

  // Regression (of the original select-reset bug, now structural): the
  // trigger label must track the RESOLVED root across async roster arrival.
  it('trigger label follows the resolved root across async roster arrival', async () => {
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
      <RootDropdown own={[]} groups={roster()} selectedKey={activeKey()} onSelect={() => {}} />
    ));

    setProjects([project('/p1', 'crucible')]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('crucible');

    setKilns([{ path: '/vault', name: 'docs' }]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('docs');

    setProjects([project('/p1', 'crucible'), project('/p2', 'other')]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('docs');
  });

  it('calls onSelect with the resolved TreeRoot when an option is picked', () => {
    const groups = buildRoster([project('/p1', 'P1')], [{ path: '/vault', name: 'Vault' }]);
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const { getByTestId } = render(() => (
      <RootDropdown own={[]} groups={groups} selectedKey={null} onSelect={onSelect} />
    ));
    openPopout(getByTestId);
    fireEvent.click(screen.getByText('Vault'));
    expect(onSelect).toHaveBeenCalledWith({ kind: 'kiln', path: '/vault', name: 'Vault' });
  });

  // The dropdown is the file pane's ONLY root control. An empty roster must
  // still render it — and it must still open, because the Clone action inside
  // is the only way out of having no roots at all.
  it('keeps an openable trigger labelled "No roots" for an empty roster', () => {
    const groups = buildRoster([], []);
    const { getByTestId } = render(() => (
      <RootDropdown own={[]} groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    const trigger = getByTestId('root-dropdown');
    expect(trigger.textContent).toContain('No roots');
    expect(trigger.hasAttribute('disabled')).toBe(false);

    openPopout(getByTestId);
    expect(screen.getByTestId('root-dropdown-action').textContent).toContain('Clone a repository');
  });

  // The strip of tabs this replaced showed the session's own roots. They are
  // now the list's first section, so one control still reaches them in one
  // open — and the roster does not repeat them further down.
  it("leads with the session's own roots and does not repeat them in the roster", () => {
    const groups = buildRoster(
      [project('/home/me/crucible', 'crucible'), project('/p2', 'other')],
      [
        { path: '/vault', name: 'Vault' },
        { path: '/archive', name: 'Archive' },
      ],
    );
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[WORKSPACE, ATTACHED]}
        groups={groups}
        selectedKey={rootKey(WORKSPACE)}
        onSelect={() => {}}
      />
    ));
    openPopout(getByTestId);
    const popout = screen.getByTestId('root-dropdown-popout');

    expect(rowLabels(popout)).toEqual(['crucible', 'Vault', 'other', 'Archive']);
    expect(popout.textContent).toContain('This session');
    // Four roots, four rows: the workspace and the attached kiln appear once
    // each, in the session section, not again under Projects/Kilns.
    expect(popout.querySelectorAll('[role="option"]')).toHaveLength(4);
  });

  // Browsing is not attaching. A root outside the session says so on its row,
  // because picking one must never read as widening what the agent can see.
  it('marks roots the session does not own as browse-only', () => {
    const groups = buildRoster([project('/home/me/crucible', 'crucible')], [
      { path: '/archive', name: 'Archive' },
    ]);
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[WORKSPACE]}
        groups={groups}
        selectedKey={rootKey(WORKSPACE)}
        onSelect={() => {}}
      />
    ));
    openPopout(getByTestId);
    const rows = [...screen.getByTestId('root-dropdown-popout').querySelectorAll('[role="option"]')];
    const hintOf = (label: string) =>
      rows.find((r) => r.textContent?.includes(label))?.textContent ?? '';
    expect(hintOf('crucible')).toContain('workspace');
    expect(hintOf('Archive')).toContain('browse only');
  });

  // A session root that is NOT a roster row (an unregistered workspace) is
  // still selectable — the roster index alone could not resolve its key.
  it('resolves a pick of a session root the roster does not list', () => {
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[WORKSPACE]}
        groups={buildRoster([], [])}
        selectedKey={null}
        onSelect={onSelect}
      />
    ));
    openPopout(getByTestId);
    fireEvent.click(screen.getByText('crucible'));
    expect(onSelect).toHaveBeenCalledWith(WORKSPACE);
  });

  it('lists workspace targets for an active project root and jumps to an existing checkout', async () => {
    branchRows = [
      { value: 'master', label: 'master', hint: 'current', path: '/repo', current: true },
      { value: 'feat/x', label: 'feat/x', hint: 'feat-x', path: '/repo/tree/feat/x' },
    ];
    registerAnswer = project('/repo/tree/feat/x', 'x');
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[]}
        groups={groups}
        selectedKey="project:/repo"
        onSelect={onSelect}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() =>
      expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('Branches — repo'),
    );

    fireEvent.click(screen.getByText('feat/x'));
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/feat/x',
        name: 'x',
      }),
    );
    expect(registered).toEqual(['/repo/tree/feat/x']);
    // A target the provider already resolved needs no round trip.
    expect(ranCommand('worktree:add')).toHaveLength(0);
  });

  // No confirmation prompt: picking a row labelled "new worktree" IS the
  // confirmation, and the provider is idempotent if it turns out to exist.
  it('asks the provider to materialise a target that has no checkout yet', async () => {
    branchRows = [{ value: 'fix/y', label: 'fix/y', hint: 'new worktree' }];
    resolveAnswer = { path: '/repo/tree/fix/y' };
    registerAnswer = project('/repo/tree/fix/y', 'y');
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[]}
        groups={groups}
        selectedKey="project:/repo"
        onSelect={onSelect}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByText('fix/y')).toBeTruthy());

    fireEvent.click(screen.getByText('fix/y'));
    await waitFor(() =>
      expect(ranCommand('worktree:add')[0]?.args).toEqual({ target: 'fix/y', workspace: '/repo' }),
    );
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/fix/y',
        name: 'y',
      }),
    );
  });

  // A target the provider refuses (a name git rejects, a busy destination)
  // must say so — this is an explicit pick, not a background enumeration.
  it('surfaces a provider refusal instead of silently doing nothing', async () => {
    branchRows = [{ value: 'fix/y', label: 'fix/y', hint: 'new worktree' }];
    resolveAnswer = { status: 500 };
    const onNotice = vi.fn();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[]}
        groups={groups}
        selectedKey="project:/repo"
        onSelect={() => {}}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
        onNotice={onNotice}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByText('fix/y')).toBeTruthy());

    fireEvent.click(screen.getByText('fix/y'));
    await waitFor(() =>
      expect(onNotice).toHaveBeenCalledWith(
        expect.stringContaining("Plugin command 'worktree:add' failed"),
      ),
    );
  });

  it('typing an unknown name offers branch-plus-worktree creation', async () => {
    branchRows = [
      { value: 'master', label: 'master', hint: 'current', path: '/repo', current: true },
    ];
    resolveAnswer = { path: '/repo/tree/feat/new-thing' };
    registerAnswer = project('/repo/tree/feat/new-thing', 'new-thing');
    const onNotice = vi.fn();
    // Big roster so the filter input renders (searchThreshold).
    const groups = buildRoster(
      Array.from({ length: 8 }, (_, i) => project(`/p${i}`, `P${i}`)),
      [],
    );
    const { getByTestId } = render(() => (
      <RootDropdown
        own={[]}
        groups={groups}
        selectedKey={null}
        onSelect={() => {}}
        activeRoot={{ kind: 'project', path: '/p0', name: 'P0' }}
        onNotice={onNotice}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() =>
      expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('master'),
    );

    const filter = screen.getByLabelText('Search Browse root') as HTMLInputElement;
    fireEvent.input(filter, { target: { value: 'feat/new-thing' } });
    const createRow = screen.getByTestId('root-dropdown-create');
    expect(createRow.textContent).toContain("Create branch + worktree 'feat/new-thing'");

    fireEvent.click(createRow);
    // Addressed to the provider that offered the other rows, so a typed name
    // goes to the same place a picked one does.
    await waitFor(() =>
      expect(ranCommand('worktree:add')[0]?.args).toEqual({
        target: 'feat/new-thing',
        workspace: '/p0',
      }),
    );
  });
});
