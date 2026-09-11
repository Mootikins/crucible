import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

// Every knob's getter below answers a value distinct from the others, so a
// control bound to the wrong knob shows the wrong number rather than passing by
// coincidence. The `timeout_secs`-vs-`execution_timeout` wire asymmetry is
// asserted on the Rust side (`routes/session_config/tests.rs`); here the value
// only has to reach the right input.
//
// `vi.hoisted`, because `vi.mock`'s factory is hoisted above ordinary top-level
// consts — referencing a plain `const` from it throws "Cannot access before
// initialization" at import time, not at assert time.
const mockSetters = vi.hoisted(() => ({
  setContextStrategy: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  getContextStrategy: vi.fn().mockResolvedValue('recent'),
  ...mockSetters,
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({ id: 's1' }),
  }),
}));

import { AdvancedSessionSettingsSection } from '../settings/AdvancedSessionSettings';

/** The section renders `<tr>`s, so it needs a table ancestor to mount into. */
function renderSection() {
  return render(() => (
    <table>
      <tbody>
        <AdvancedSessionSettingsSection />
      </tbody>
    </table>
  ));
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('AdvancedSessionSettings', () => {
  it('sends the enum knob by its string spelling', async () => {
    renderSection();
    await waitFor(() => screen.getByTestId('context-strategy-select'));

    fireEvent.change(screen.getByTestId('context-strategy-select'), {
      target: { value: 'truncate' },
    });
    await waitFor(() =>
      expect(mockSetters.setContextStrategy).toHaveBeenCalledWith('s1', 'truncate'),
    );
  });

  it('keeps a strategy name the dropdown does not know about', async () => {
    // The daemon owns the enum; a value it accepts must not vanish from the UI
    // because this file's convenience list is out of date. Nothing here
    // validates — the daemon answers 422 for a name it rejects.
    const api = await import('@/lib/api');
    vi.mocked(api.getContextStrategy).mockResolvedValueOnce('some-future-strategy');

    renderSection();

    await waitFor(() =>
      expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
        'some-future-strategy',
      ),
    );
  });
});
