import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Session } from '@/lib/types';

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    // Typed against the full Session contract (types.ts) so the mock can't
    // silently drift from the daemon payload — the return annotation forces
    // every required field (agent_mode) plus the optional last_activity /
    // archived to be present.
    currentSession: (): Session => ({
      session_id: 's1',
      type: 'chat',
      kilns: ['/tmp/k'],
      workspace: '/tmp/k',
      state: 'active',
      title: null,
      agent_model: null,
      started_at: '',
      last_activity: null,
      event_count: 0,
      archived: false,
    }),
  }),
}));

// None of the three skills calls is stubbed: the panel reads them through
// `useSkillList`, `useSkillSearch` and `useSkillDetail`, which run the real
// functions against the mocked fetch below. `getConfig` stays a stub because
// the kiln resolver awaits it outside the query layer.
const getConfigMock = vi.fn().mockResolvedValue({ kiln_path: '/tmp/k' });

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getConfig: () => getConfigMock(),
}));

const addNotificationMock = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...args: unknown[]) => addNotificationMock(...args) },
}));

// Import after mocks.
import { SkillsPanel } from '../SkillsPanel';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { apiError, type MockFetchHandler } from '@/test-utils/mock-fetch';
import { resetKilnsForTests } from '@/lib/query/kilns';
import type { SkillSummary } from '@/lib/types';

// The panel resolves a kiln NAME to a directory through `kilnPathOf`, which
// reads the shared kiln query. An empty registry is the case under test:
// nothing claims the session's name, `kilnPathOf` answers null, and the panel
// falls back to the configured `kiln_path`.
let env: TestQueryEnv;

/** The roster `GET /api/skills` answers next. */
let listed: SkillSummary[] = [];
/** The hits `GET /api/skills/search` answers next. */
let found: SkillSummary[] = [];
/** A refusal to serve in place of the roster, once. */
let listRefusal: MockFetchHandler | null = null;

const ALPHA_DETAIL = {
  name: 'alpha',
  scope: 'user',
  description: 'first',
  source_path: '/tmp/alpha.md',
  agent: null,
  license: null,
  body: '# Alpha\n\nThe body.',
};

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  listed = [
    { name: 'alpha', scope: 'user', description: 'first', shadowed_count: 0 },
    { name: 'beta', scope: 'user', description: 'second', shadowed_count: 1 },
    { name: 'gamma', scope: 'kiln', description: 'kiln-local', shadowed_count: 0 },
  ];
  found = [{ name: 'beta', scope: 'user', description: 'second', shadowed_count: 0 }];
  listRefusal = null;
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [] }),
    'GET /api/skills': () => {
      const refusal = listRefusal;
      listRefusal = null;
      return refusal ? new Response(JSON.stringify(refusal.body), { status: refusal.status }) : { skills: listed };
    },
    'GET /api/skills/search': () => ({ skills: found }),
    'GET /api/skills/alpha': () => ALPHA_DETAIL,
  });
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
});

describe('SkillsPanel', () => {
  beforeEach(() => {
    addNotificationMock.mockClear();
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('groups skills by scope and renders rows', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(env.fetch.calls('GET /api/skills')).toBe(1));

    await waitFor(() => {
      expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument();
      expect(screen.getByTestId('skill-row-beta')).toBeInTheDocument();
      expect(screen.getByTestId('skill-row-gamma')).toBeInTheDocument();
    });

    // Scope headers visible (grouping).
    expect(screen.getByText('user')).toBeInTheDocument();
    expect(screen.getByText('kiln')).toBeInTheDocument();
  });

  it('shows the shadow badge when shadowed_count > 0', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByText('+1')).toBeInTheDocument());
  });

  it('debounces typed query and switches to search endpoint', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());

    const input = screen.getByTestId('skills-search-input') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'be' } });

    // Before debounce fires, search shouldn't be called.
    expect(env.fetch.calls('GET /api/skills/search')).toBe(0);

    // Advance past the 200ms debounce.
    vi.advanceTimersByTime(250);
    await waitFor(() => expect(env.fetch.calls('GET /api/skills/search')).toBe(1));
    await waitFor(() => expect(screen.getByTestId('skill-row-beta')).toBeInTheDocument());
    expect(screen.queryByTestId('skill-row-alpha')).not.toBeInTheDocument();

    // The roster stayed in the cache while the search ran, so clearing the box
    // draws it again without a second GET.
    fireEvent.input(input, { target: { value: '' } });
    vi.advanceTimersByTime(250);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());
    expect(env.fetch.calls('GET /api/skills')).toBe(1);
  });

  it('opens the drawer and loads detail on row click', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-drawer')).toBeInTheDocument());
    await waitFor(() => expect(env.fetch.calls('GET /api/skills/alpha')).toBe(1));
    await waitFor(() => {
      const drawer = screen.getByTestId('skills-drawer');
      const pre = drawer.querySelector('pre');
      expect(pre?.textContent).toContain('# Alpha');
      expect(pre?.textContent).toContain('The body');
    });
  });

  it('copy-invocation writes /<name> to clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-copy-invocation')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skills-copy-invocation'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('/alpha'));
    expect(addNotificationMock).toHaveBeenCalledWith('success', expect.stringContaining('/alpha'));
  });

  it('drawer close returns to list', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-drawer')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skills-drawer-close'));
    await waitFor(() => expect(screen.queryByTestId('skills-drawer')).not.toBeInTheDocument());
  });

  // The old fetcher caught its own failure and answered an empty list, which
  // drew "No skills discovered." over a kiln the daemon had refused to read.
  // The notification is the only thing that told the two apart, so it has to
  // survive the move to the query layer.
  it('reports the daemon refusal rather than drawing an empty roster', async () => {
    listRefusal = apiError(500, 'the kiln is not readable');
    render(() => <SkillsPanel />);
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith(
        'error',
        expect.stringContaining('Failed to list skills'),
      ),
    );
  });
});
