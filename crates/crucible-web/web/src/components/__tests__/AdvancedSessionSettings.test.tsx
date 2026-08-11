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
  setContextBudget: vi.fn(),
  setContextWindow: vi.fn(),
  setAutocompactThreshold: vi.fn(),
  setMaxIterations: vi.fn(),
  setExecutionTimeout: vi.fn(),
  setValidationRetries: vi.fn(),
  setContextStrategy: vi.fn(),
  setOutputValidation: vi.fn(),
  setSystemPrompt: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  getContextBudget: vi.fn().mockResolvedValue(111),
  getContextWindow: vi.fn().mockResolvedValue(222),
  getAutocompactThreshold: vi.fn().mockResolvedValue(0.75),
  getMaxIterations: vi.fn().mockResolvedValue(33),
  getExecutionTimeout: vi.fn().mockResolvedValue(44),
  getValidationRetries: vi.fn().mockResolvedValue(5),
  getContextStrategy: vi.fn().mockResolvedValue('recent'),
  getOutputValidation: vi.fn().mockResolvedValue('strict'),
  getSystemPrompt: vi.fn().mockResolvedValue('be terse'),
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
  it('loads every knob into its own control', async () => {
    renderSection();

    await waitFor(() =>
      expect((screen.getByTestId('context-budget-input') as HTMLInputElement).value).toBe('111'),
    );
    expect((screen.getByTestId('context-window-input') as HTMLInputElement).value).toBe('222');
    expect((screen.getByTestId('autocompact-threshold-input') as HTMLInputElement).value).toBe(
      '0.75',
    );
    expect((screen.getByTestId('max-iterations-input') as HTMLInputElement).value).toBe('33');
    expect((screen.getByTestId('execution-timeout-input') as HTMLInputElement).value).toBe('44');
    expect((screen.getByTestId('validation-retries-input') as HTMLInputElement).value).toBe('5');
    expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
      'recent',
    );
    expect((screen.getByTestId('output-validation-select') as HTMLSelectElement).value).toBe(
      'strict',
    );
    expect((screen.getByTestId('system-prompt-input') as HTMLTextAreaElement).value).toBe(
      'be terse',
    );
  });

  it('commits each numeric knob on blur, to its own setter', async () => {
    renderSection();
    await waitFor(() => screen.getByTestId('context-budget-input'));

    const cases: [string, keyof typeof mockSetters, string, number][] = [
      ['context-budget-input', 'setContextBudget', '8000', 8000],
      ['context-window-input', 'setContextWindow', '32000', 32000],
      ['max-iterations-input', 'setMaxIterations', '12', 12],
      ['execution-timeout-input', 'setExecutionTimeout', '300', 300],
      ['validation-retries-input', 'setValidationRetries', '3', 3],
    ];

    for (const [testId, setter, typed, expected] of cases) {
      const input = screen.getByTestId(testId);
      fireEvent.input(input, { target: { value: typed } });
      fireEvent.blur(input);
      await waitFor(() => expect(mockSetters[setter]).toHaveBeenCalledWith('s1', expected));
    }
  });

  it('clearing an optional knob sends null, meaning restore the default', async () => {
    renderSection();
    await waitFor(() => screen.getByTestId('context-budget-input'));

    const input = screen.getByTestId('context-budget-input');
    fireEvent.input(input, { target: { value: '' } });
    fireEvent.blur(input);

    // `null`, not `0` and not "no call at all": absent and zero are different
    // instructions to the daemon.
    await waitFor(() => expect(mockSetters.setContextBudget).toHaveBeenCalledWith('s1', null));
  });

  it('leaves an empty validation-retries alone, because the knob is not nullable', async () => {
    renderSection();
    await waitFor(() => screen.getByTestId('validation-retries-input'));

    const input = screen.getByTestId('validation-retries-input');
    fireEvent.input(input, { target: { value: '' } });
    fireEvent.blur(input);

    expect(mockSetters.setValidationRetries).not.toHaveBeenCalled();
  });

  it('sends the enum knobs by their string spelling', async () => {
    renderSection();
    await waitFor(() => screen.getByTestId('context-strategy-select'));

    fireEvent.change(screen.getByTestId('context-strategy-select'), {
      target: { value: 'truncate' },
    });
    await waitFor(() =>
      expect(mockSetters.setContextStrategy).toHaveBeenCalledWith('s1', 'truncate'),
    );

    fireEvent.change(screen.getByTestId('output-validation-select'), {
      target: { value: 'lenient' },
    });
    await waitFor(() =>
      expect(mockSetters.setOutputValidation).toHaveBeenCalledWith('s1', 'lenient'),
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

  it('debounces the system prompt instead of firing a PUT per keystroke', async () => {
    vi.useFakeTimers();
    try {
      renderSection();
      await vi.waitFor(() => screen.getByTestId('system-prompt-input'));

      const box = screen.getByTestId('system-prompt-input');
      for (const text of ['a', 'ab', 'abc']) {
        fireEvent.input(box, { target: { value: text } });
      }
      expect(mockSetters.setSystemPrompt).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(500);
      expect(mockSetters.setSystemPrompt).toHaveBeenCalledTimes(1);
      expect(mockSetters.setSystemPrompt).toHaveBeenCalledWith('s1', 'abc');
    } finally {
      vi.useRealTimers();
    }
  });
});
