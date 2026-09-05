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

vi.mock('@/lib/api', () => ({
  listKnobs: (...a: unknown[]) => listKnobs(...a),
  getThinkingBudget: vi.fn(async () => 8192),
  setThinkingBudget: vi.fn(async () => {}),
  getTemperature: vi.fn(async () => 0.7),
  setTemperature: vi.fn(async () => {}),
  getMaxTokens: vi.fn(async () => 4096),
  setMaxTokens: vi.fn(async () => {}),
  getPrecognition: vi.fn(async () => true),
  setPrecognition: vi.fn(async () => {}),
  getPrecognitionResults: vi.fn(async () => 5),
  setPrecognitionResults: vi.fn(async () => {}),
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
  knobs: [
    { id: 'thinking_budget', supported: true },
    { id: 'temperature', supported: true },
    { id: 'max_tokens', supported: true },
    { id: 'precognition', supported: true },
  ],
};

/** What it answers for an ACP session: the protocol carries none of these. */
const ACP_SESSION = {
  knobs: [
    { id: 'thinking_budget', supported: false },
    { id: 'temperature', supported: false },
    { id: 'max_tokens', supported: false },
    { id: 'precognition', supported: true },
  ],
};

beforeEach(() => {
  listKnobs.mockResolvedValue(ALL_SUPPORTED);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('ModelSettingsSection', () => {
  it('draws the controls a session supports', async () => {
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(listKnobs).toHaveBeenCalledWith('s1'));
    await waitFor(() => expect(screen.getByText('Temperature')).toBeTruthy());
    expect(screen.getByText('Thinking Budget')).toBeTruthy();
    expect(screen.getByText('Max Tokens')).toBeTruthy();
  });

  it('draws no control for a setting the session does not have', async () => {
    listKnobs.mockResolvedValue(ACP_SESSION);
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(listKnobs).toHaveBeenCalledWith('s1'));
    // Precognition is supported, so its arrival is what says the answer landed
    // — without it this could pass against a panel that never rendered at all.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());

    expect(screen.queryByText('Temperature')).toBeNull();
    expect(screen.queryByText('Thinking Budget')).toBeNull();
    expect(screen.queryByText('Max Tokens')).toBeNull();
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

    // Wait for a row that is never gated, so the absence below is a decision
    // and not a render that has yet to happen. Asserting straight after the
    // call was made passed against a panel with no gating at all.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());
    expect(screen.queryByText('Temperature')).toBeNull();
    expect(screen.queryByText('Max Tokens')).toBeNull();
    expect(screen.queryByText('Thinking Budget')).toBeNull();
  });
});
