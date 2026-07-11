import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@solidjs/testing-library';
import InboxPanel from '../InboxPanel';
import { attentionStore, attentionActions } from '@/stores/attentionStore';
import type { PermRequest } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  respondToInteraction: vi.fn().mockResolvedValue(undefined),
  listPendingInteractions: vi.fn().mockResolvedValue([]),
}));

import { respondToInteraction } from '@/lib/api';

const perm: PermRequest = {
  kind: 'permission',
  id: 'req-42',
  action_type: 'bash',
  tokens: ['cargo', 'test', '--package', 'helios-core'],
  tool_name: 'Bash',
};

function clearAttention() {
  for (const id of Object.keys(attentionStore.entries)) {
    attentionActions.clear(id);
  }
}

beforeEach(clearAttention);
afterEach(() => {
  cleanup();
  clearAttention();
  vi.clearAllMocks();
});

describe('InboxPanel', () => {
  it('shows all-clear when nothing is pending', () => {
    const { getByText } = render(() => <InboxPanel />);
    expect(getByText(/all clear/)).toBeTruthy();
    expect(getByText(/0 pending/)).toBeTruthy();
  });

  it('renders a pending permission answerable in place', () => {
    attentionActions.report('s1', {
      pendingInteraction: perm,
      title: 'scheduler-backpressure',
    });

    const { getByText, queryByText } = render(() => <InboxPanel />);
    expect(getByText('scheduler-backpressure')).toBeTruthy();
    expect(getByText(/cargo test --package helios-core/)).toBeTruthy();
    expect(getByText(/1 pending/)).toBeTruthy();
    expect(queryByText(/all clear/)).toBeNull();
  });

  it('responds via the API and broadcasts resolution on Allow', async () => {
    attentionActions.report('s1', {
      pendingInteraction: perm,
      title: 'scheduler-backpressure',
    });
    const resolvedEvents: Array<{ sessionId: string; requestId: string }> = [];
    const onResolved = (e: Event) =>
      resolvedEvents.push((e as CustomEvent<{ sessionId: string; requestId: string }>).detail);
    window.addEventListener('crucible:interaction-resolved', onResolved);

    const { getByText } = render(() => <InboxPanel />);
    (getByText('Allow') as HTMLElement).click();

    await waitFor(() => {
      expect(respondToInteraction).toHaveBeenCalledWith(
        's1',
        'req-42',
        expect.objectContaining({ allowed: true })
      );
    });
    expect(resolvedEvents).toEqual([{ sessionId: 's1', requestId: 'req-42' }]);
    // Entry resolved locally: badge drops, resolved note shows.
    await waitFor(() => {
      expect(attentionStore.attentionCount()).toBe(0);
      expect(getByText(/Resolved — scheduler-backpressure/)).toBeTruthy();
    });

    window.removeEventListener('crucible:interaction-resolved', onResolved);
  });
});
