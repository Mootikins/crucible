import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { apiError, type MockFetchHandler } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { keys } from '../keys';
import {
  useTargetProviders,
  useAxisTargets,
  useWorkspaceTargets,
  useWorkspaceTargetsByRoot,
  useResolveWorkspaceTarget,
} from '../targets';

/** What `GET /api/plugins/publications` answers: two providers, one per axis. */
const PUBLICATIONS = {
  publications: {
    targets: {
      worktree: {
        axis: 'workspace',
        label: 'Worktree',
        targets_command: 'worktree:list',
        resolve_command: 'worktree:add',
      },
      oci: { axis: 'runtime', label: 'Container', targets_command: 'oci:list' },
    },
  },
};

/** One command call the test recorded: the command name and its arguments. */
interface CommandCall {
  name: string;
  args: { workspace?: string; target?: string };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
let commands: CommandCall[] = [];

/**
 * The plugin-command route, answering per command name.
 *
 * `worktree:list` answers one target named after the workspace it was asked
 * about, so a test can tell an answer for one project from an answer for
 * another.
 */
function commandRoute(): (request: Request) => Promise<unknown> {
  return async (request: Request) => {
    const body = (await request.json()) as CommandCall;
    commands.push(body);
    const workspace = body.args?.workspace ?? 'none';
    if (body.name === 'worktree:add') return { path: `/checkout/${body.args?.target}` };
    return { targets: [{ value: workspace, label: `branch of ${workspace}` }] };
  };
}

/** The two routes every target read goes through. */
function targetRoutes(publications: unknown = PUBLICATIONS) {
  return {
    'GET /api/plugins/publications': () => publications as MockFetchHandler,
    'POST /api/plugins/command': commandRoute(),
  };
}

/** The commands one plugin was asked to run. */
const ranCommand = (name: string) => commands.filter((c) => c.name === name);

beforeEach(() => {
  commands = [];
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useTargetProviders', () => {
  it('fetches once for two callers on one axis', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      first: useTargetProviders('workspace'),
      second: useTargetProviders('workspace'),
    }));

    await vi.waitFor(() => expect(both.first.data?.map((p) => p.plugin)).toEqual(['worktree']));
    expect(both.second.data?.map((p) => p.plugin)).toEqual(['worktree']);
    expect(env.fetch.calls('GET /api/plugins/publications')).toBe(1);
  });

  it('keeps the two axes apart', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      workspace: useTargetProviders('workspace'),
      runtime: useTargetProviders('runtime'),
    }));

    await vi.waitFor(() => expect(both.workspace.data?.map((p) => p.plugin)).toEqual(['worktree']));
    await vi.waitFor(() => expect(both.runtime.data?.map((p) => p.plugin)).toEqual(['oci']));
  });

  it('surfaces a refusal as an error rather than as no providers', async () => {
    env = createTestQueryEnv({
      'GET /api/plugins/publications': apiError(500, 'the plugin host is down'),
    });

    const query = inRoot(() => useTargetProviders('workspace'));

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.data).toBeUndefined();
  });
});

describe('useAxisTargets', () => {
  it('asks each provider once, however many callers read the axis', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      first: useAxisTargets('workspace', () => '/repo/a'),
      second: useAxisTargets('workspace', () => '/repo/a'),
    }));

    await vi.waitFor(() => expect(both.first.ready).toBe(true));
    expect(both.first.targets.worktree?.map((t) => t.spec)).toEqual(['worktree:/repo/a']);
    await vi.waitFor(() => expect(both.second.targets.worktree?.[0]?.value).toBe('/repo/a'));
    expect(ranCommand('worktree:list')).toHaveLength(1);
    expect(env.fetch.calls('GET /api/plugins/publications')).toBe(1);
  });

  it('re-keys on the project, so one project never answers for another', async () => {
    env = createTestQueryEnv(targetRoutes());
    const [workspace, setWorkspace] = createSignal<string | undefined>('/repo/a');

    const axis = inRoot(() => useAxisTargets('workspace', workspace));

    await vi.waitFor(() => expect(axis.targets.worktree?.[0]?.value).toBe('/repo/a'));
    expect(ranCommand('worktree:list')).toHaveLength(1);

    // A second project is a second key, so it asks again — and what it paints
    // is the new project's answer, never the one left behind.
    setWorkspace('/repo/b');
    await vi.waitFor(() => expect(axis.targets.worktree?.[0]?.value).toBe('/repo/b'));
    expect(ranCommand('worktree:list')[1]?.args.workspace).toBe('/repo/b');
  });

  it('is ready with no targets when the axis has no provider', async () => {
    env = createTestQueryEnv(targetRoutes({ publications: { targets: {} } }));

    const axis = inRoot(() => useAxisTargets('workspace', () => '/repo/a'));

    await vi.waitFor(() => expect(axis.ready).toBe(true));
    expect(axis.targets).toEqual({});
    expect(commands).toHaveLength(0);
  });
});

describe('useWorkspaceTargets', () => {
  it('fetches once per project, and shares one project between two readers', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      first: useWorkspaceTargets(() => '/repo/a'),
      second: useWorkspaceTargets(() => '/repo/a'),
    }));

    await vi.waitFor(() => expect(both.first.data?.[0]?.value).toBe('/repo/a'));
    expect(both.second.data?.[0]?.value).toBe('/repo/a');
    expect(ranCommand('worktree:list')).toHaveLength(1);
  });

  it('asks nothing until it has a project', async () => {
    env = createTestQueryEnv(targetRoutes());

    const query = inRoot(() => useWorkspaceTargets(() => undefined));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls('GET /api/plugins/publications')).toBe(0);
  });
});

describe('useWorkspaceTargetsByRoot', () => {
  it('fans out once per root, and joins the read another caller started', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      byRoot: useWorkspaceTargetsByRoot(() => ['/repo/a', '/repo/b']),
      single: useWorkspaceTargets(() => '/repo/a'),
    }));

    await vi.waitFor(() => expect(both.byRoot().size).toBe(2));
    expect(both.byRoot().get('/repo/a')?.[0]?.value).toBe('/repo/a');
    expect(both.byRoot().get('/repo/b')?.[0]?.value).toBe('/repo/b');
    await vi.waitFor(() => expect(both.single.data?.[0]?.value).toBe('/repo/a'));
    // Two roots, two fan-outs — and the single reader joined the first one.
    expect(ranCommand('worktree:list')).toHaveLength(2);
  });
});

describe('useResolveWorkspaceTarget', () => {
  it('answers the checkout path, and asks that project for its targets again', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      targets: useWorkspaceTargets(() => '/repo/a'),
      resolve: useResolveWorkspaceTarget(),
    }));

    await vi.waitFor(() => expect(both.targets.data?.[0]?.value).toBe('/repo/a'));
    expect(ranCommand('worktree:list')).toHaveLength(1);
    expect(env.client.getQueryData(keys.workspaceTargets('/repo/a'))).toBeDefined();

    const path = await both.resolve.mutateAsync({ spec: 'worktree:fix/y', workspace: '/repo/a' });

    expect(path).toBe('/checkout/fix/y');
    // The list the caller just read said this target had no checkout. It has
    // one now, so that project's entry is asked again.
    await vi.waitFor(() => expect(ranCommand('worktree:list')).toHaveLength(2));
  });

  it('asks again for the per-provider lists the composers read', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      axis: useAxisTargets('workspace', () => '/repo/a'),
      resolve: useResolveWorkspaceTarget(),
    }));

    await vi.waitFor(() => expect(both.axis.ready).toBe(true));
    expect(ranCommand('worktree:list')).toHaveLength(1);

    await both.resolve.mutateAsync({ spec: 'worktree:fix/y', workspace: '/repo/a' });

    // The composer's picker is keyed per provider, not under the flat
    // workspace key the files pane reads. A new checkout makes both wrong.
    await vi.waitFor(() => expect(ranCommand('worktree:list')).toHaveLength(2));
  });

  it('leaves the lists of another project alone', async () => {
    env = createTestQueryEnv(targetRoutes());

    const both = inRoot(() => ({
      other: useAxisTargets('workspace', () => '/repo/b'),
      resolve: useResolveWorkspaceTarget(),
    }));

    await vi.waitFor(() => expect(both.other.ready).toBe(true));
    expect(ranCommand('worktree:list')).toHaveLength(1);

    await both.resolve.mutateAsync({ spec: 'worktree:fix/y', workspace: '/repo/a' });

    await vi.waitFor(() => expect(ranCommand('worktree:add')).toHaveLength(1));
    expect(ranCommand('worktree:list')).toHaveLength(1);
  });

  it('reports a provider that resolves nothing, rather than doing nothing', async () => {
    env = createTestQueryEnv({
      'GET /api/plugins/publications': () => PUBLICATIONS as MockFetchHandler,
      'POST /api/plugins/command': () => ({ path: null }),
    });

    const resolve = inRoot(() => useResolveWorkspaceTarget());

    await expect(
      resolve.mutateAsync({ spec: 'worktree:fix/y', workspace: '/repo/a' }),
    ).rejects.toThrow(/resolved 'worktree:fix\/y' to no path/);
  });
});
