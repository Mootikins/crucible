import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

const created = vi.hoisted(() => ({ params: [] as unknown[], opts: [] as unknown[] }));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    createSession: (params: unknown, opts: unknown) => {
      created.params.push(params);
      created.opts.push(opts);
      return Promise.resolve({ id: 's-new' });
    },
  }),
}));
// Neither `listKilns` nor `listAgents` is stubbed: the sheet reads both
// rosters through their query hooks, which run the real functions against the
// mocked fetch below.
vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getConfig: () => Promise.resolve({ kiln_path: '/kilns/home' }),
  listAllModels: () => Promise.resolve(['sonnet', 'opus']),
  listProjects: () => Promise.resolve([{ path: '/work/alpha', name: 'alpha', kilns: [] }]),
  getTargetProviders: () => Promise.resolve([]),
  getProviderTargets: () => Promise.resolve([]),
}));
vi.mock('@/lib/draft-session', () => ({ closeDraftTab: vi.fn() }));

import { NewSessionSheet } from '@/components/mobile/NewSessionSheet';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

const KILNS = [{ path: '/kilns/home', name: 'home' }];
const AGENTS = [
  { name: 'claude', description: 'ACP agent', command: 'npx', is_builtin: true, available: true },
];

let env: TestQueryEnv;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  localStorage.setItem('crucible:cache:kilns', JSON.stringify(KILNS));
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: KILNS }),
    'GET /api/agents': () => ({ agents: AGENTS }),
  });
  created.params = [];
  created.opts = [];
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
});

describe('NewSessionSheet', () => {
  it('leads with the agents, so a user sees what each one is', async () => {
    render(() => <NewSessionSheet />);
    expect(screen.getByRole('heading', { name: 'Agent' })).toBeTruthy();
    // The internal agent is the default, and it is first.
    const cards = await waitFor(() => screen.getAllByRole('radio'));
    expect(cards[0].textContent).toContain('Crucible');
    expect(cards[0].getAttribute('aria-checked')).toBe('true');
  });

  it('shows the five context rows, not four', async () => {
    render(() => <NewSessionSheet />);
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    for (const row of ['Project', 'Workspace', 'Kiln', 'Model', 'Runtime']) {
      expect(await waitFor(() => screen.getByRole('button', { name: new RegExp(`^${row}`) }))).toBeTruthy();
    }
  });

  it('creates the session with the first message, and says nothing it was not told', async () => {
    render(() => <NewSessionSheet />);
    // No card field: a card names a subagent or an @-callout, not a draft.
    expect(screen.queryByRole('textbox', { name: 'Agent card' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    const box = screen.getByRole('textbox', { name: 'Message' });
    fireEvent.input(box, { target: { value: 'hello there' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(created.params).toHaveLength(1));
    const params = created.params[0] as Record<string, unknown>;
    expect(created.opts[0]).toMatchObject({ initialMessage: 'hello there' });
    expect(params).not.toHaveProperty('agent_card');
    expect(params.agent_name).toBeUndefined();
    // Untouched axes say nothing at all.
    expect('isolation' in params).toBe(false);
    expect('workspace_target' in params).toBe(false);
    expect('agent_type' in params).toBe(false);
  });

  it('refuses to send an empty message', () => {
    render(() => <NewSessionSheet />);
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByRole('button', { name: 'Send' }).hasAttribute('disabled')).toBe(true);
  });

  it('opens aimed at a project when one was named', async () => {
    render(() => <NewSessionSheet workspace="/work/alpha" />);
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /^Project/ }).textContent).toContain('alpha'),
    );
  });
});
