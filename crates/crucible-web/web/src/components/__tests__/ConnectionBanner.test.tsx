import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@solidjs/testing-library';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ConnectionBanner } from '../ui/ConnectionBanner';

/**
 * Three surfaces can lose their link to the daemon — the PTY socket, the chat
 * event stream and a file save — and before this only ONE of them offered a way
 * out. The banner is the single affordance; these tests hold both halves of
 * that: the component behaves, and all three surfaces actually use it.
 */

const SRC = resolve(__dirname, '../..');
const read = (rel: string) => readFileSync(resolve(SRC, rel), 'utf-8');

afterEach(cleanup);

describe('ConnectionBanner', () => {
  it('states the fault and offers the retry', () => {
    const retry = vi.fn();
    render(() => (
      <ConnectionBanner
        tone="transient"
        message="Reconnecting…"
        retryLabel="Retry now"
        onRetry={retry}
        testid="b"
        retryTestid="b-retry"
      />
    ));

    expect(screen.getByTestId('b')).toHaveTextContent('Reconnecting…');
    fireEvent.click(screen.getByTestId('b-retry'));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it('renders NO control when the surface has nothing to re-issue', () => {
    // The whole point of the optional `onRetry`: a button that cannot recover
    // anything is worse than no button, because it teaches the user that this
    // fault class is unrecoverable when elsewhere it is not.
    render(() => <ConnectionBanner tone="error" message="Gone" testid="b" retryTestid="b-retry" />);
    expect(screen.queryByTestId('b-retry')).toBeNull();
    expect(screen.getByTestId('b')).toHaveTextContent('Gone');
  });

  it('announces politely rather than interrupting', () => {
    // A socket that heals on its own timer must not cut across a screen
    // reader mid-sentence, so this is `status`, never `alert`.
    render(() => <ConnectionBanner tone="transient" message="Reconnecting…" testid="b" />);
    const banner = screen.getByTestId('b');
    expect(banner).toHaveAttribute('role', 'status');
    expect(banner).toHaveAttribute('aria-live', 'polite');
  });

  it('carries the tone as data, so a surface cannot silently pick the wrong one', () => {
    render(() => <ConnectionBanner tone="error" message="x" testid="b" />);
    expect(screen.getByTestId('b')).toHaveAttribute('data-tone', 'error');
  });

  it('wears the one focus treatment on its control', () => {
    render(() => (
      <ConnectionBanner tone="error" message="x" onRetry={() => {}} retryTestid="b-retry" />
    ));
    const button = screen.getByTestId('b-retry');
    expect(button.className).toContain('focus-ring');
    // `focus-ring` REPLACES outline-none; the two are never written together.
    expect(button.className).not.toContain('outline-none');
  });
});

describe('one recovery affordance, three surfaces', () => {
  // The defect was three surfaces naming the same class of fault and one of
  // them acting on it. Each of these files must reach the shared banner; a
  // fourth hand-rolled "Reconnecting…" strip is the regression.
  const SURFACES = ['components/TerminalPanel.tsx', 'components/ChatInput.tsx', 'components/EditorPanel.tsx'];

  for (const surface of SURFACES) {
    it(`${surface} renders the shared banner`, () => {
      const src = read(surface);
      expect(src).toMatch(/import \{ ConnectionBanner \} from '@\/components\/ui\/ConnectionBanner'/);
      expect(src).toMatch(/<ConnectionBanner\b/);
    });
  }
});
