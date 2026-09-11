import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@solidjs/testing-library';

/**
 * The panel drew a fixed set of controls, which was wrong for every ACP
 * session. ACP has no temperature and no token cap — the protocol has no
 * field for either — so the slider and the number box changed nothing, and
 * the daemon now refuses those calls outright.
 *
 * So the panel asks the session what it has. These tests drive the two
 * answers a session can give and check what gets drawn, because that is where
 * the old bug was visible and nowhere else.
 */
const listKnobs = vi.fn();
const listAgentOptions = vi.fn();
const setAgentOption = vi.fn();

vi.mock('@/lib/api', () => ({
  listKnobs: (...a: unknown[]) => listKnobs(...a),
  listAgentOptions: (...a: unknown[]) => listAgentOptions(...a),
  setAgentOption: (...a: unknown[]) => setAgentOption(...a),
  getPrecognition: vi.fn(async () => true),
  setPrecognition: vi.fn(async () => {}),
  getPlugins: vi.fn(async () => []),
  reloadPlugin: vi.fn(async () => {}),
  getMcpStatus: vi.fn(async () => ({ servers: [] })),
  login: vi.fn(async () => true),
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({ id: 's1', title: 'T' }),
  }),
}));

import { ModelSettingsSection } from '../SettingsPanel';

/** What the daemon answers for a session that has every setting. */
const ALL_SUPPORTED = {
  knobs: [{ id: 'precognition', supported: true }],
};

/**
 * A session that refuses it. Which knobs a session refuses depends on the
 * session, so what these tests pin is that the panel obeys the answer it is
 * given rather than a set of rows compiled into the client.
 */
const ONE_UNSUPPORTED = {
  knobs: [{ id: 'precognition', supported: false }],
};

beforeEach(() => {
  listKnobs.mockResolvedValue(ALL_SUPPORTED);
  listAgentOptions.mockResolvedValue({ options: [] });
  setAgentOption.mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('ModelSettingsSection', () => {
  it('draws the controls a session supports', async () => {
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(listKnobs).toHaveBeenCalledWith('s1'));
    // `waitFor`, not a bare assertion: the call landing is not the render
    // landing, and asserting between the two passes against a panel that
    // never drew anything.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());
  });

  it('draws no control for a setting the session does not have', async () => {
    listKnobs.mockResolvedValue(ONE_UNSUPPORTED);
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(listKnobs).toHaveBeenCalledWith('s1'));
    // The agent-options loop is never gated on the knob list, so waiting on it
    // means the absence below is a decision rather than a render that has yet
    // to happen.
    await waitFor(() => expect(listAgentOptions).toHaveBeenCalledWith('s1'));

    expect(screen.queryByText('Precognition')).toBeNull();
  });

  it('draws nothing rather than guessing when the answer lists nothing', async () => {
    // A daemon that answers but names no knob — an older one, or one whose
    // list grew a name this client does not know. A control drawn on a guess
    // is the bug this whole change is about, so absence hides it.
    //
    // Stated as an empty list rather than a failed call on purpose: a failed
    // call is hidden by the panel's error path, so it would pass with no
    // gating at all and prove nothing.
    listKnobs.mockResolvedValue({ knobs: [] });
    render(() => <ModelSettingsSection />);

    // Wait for the agent-options loop, which is never gated on the knob list,
    // so the absence below is a decision and not a render that has yet to
    // happen. Asserting straight after the call was made passed against a
    // panel with no gating at all.
    await waitFor(() => expect(listAgentOptions).toHaveBeenCalledWith('s1'));

    expect(screen.queryByText('Precognition')).toBeNull();
  });
});

/**
 * The agent's own settings. Crucible has no knob for these — a different
 * agent advertises different ones — so the panel renders what it is given
 * rather than a set of named rows. That generality is the whole point: an
 * option this file has never heard of has to draw and work.
 */
describe('ModelSettingsSection agent options', () => {
  const REASONING = {
    id: 'thought_level',
    name: 'Reasoning',
    description: 'How long the agent thinks',
    category: 'thought_level',
    kind: 'select' as const,
    current: 'low',
    choices: [
      { value: 'low', name: 'Low' },
      { value: 'high', name: 'High' },
    ],
  };

  it('draws an option it has never heard of, and sends the choice back', async () => {
    listAgentOptions.mockResolvedValue({ options: [REASONING] });
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(screen.getByText('Reasoning')).toBeTruthy());
    const select = screen.getByTestId('agent-option-thought_level') as HTMLSelectElement;
    expect(select.value).toBe('low');
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['low', 'high']);

    select.value = 'high';
    select.dispatchEvent(new Event('change', { bubbles: true }));

    await waitFor(() =>
      expect(setAgentOption).toHaveBeenCalledWith('s1', 'thought_level', 'high'),
    );
  });

  it('draws a toggle for a boolean option', async () => {
    listAgentOptions.mockResolvedValue({
      options: [
        { id: 'verbose', name: 'Verbose', description: null, category: null, kind: 'toggle', current: false },
      ],
    });
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(screen.getByTestId('agent-option-verbose')).toBeTruthy());
  });

  it('draws nothing when the agent advertised nothing', async () => {
    listAgentOptions.mockResolvedValue({ options: [] });
    render(() => <ModelSettingsSection />);

    // Anchor on an ungated row so the absence is a decision, not a pending
    // render.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());
    expect(screen.queryByText('Reasoning')).toBeNull();
  });
});
