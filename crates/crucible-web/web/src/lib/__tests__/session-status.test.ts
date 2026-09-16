import { describe, it, expect, beforeEach } from 'vitest';
import { sessionStatus, STATUS_RANK } from '@/lib/session-status';
import { attentionActions } from '@/stores/attentionStore';

const ASK = { id: 'req-1', kind: 'ask' as const, question: 'Proceed?' };

describe('sessionStatus', () => {
  beforeEach(() => {
    for (const id of ['s1', 's2', 's3']) attentionActions.clear(id);
  });

  it('reads a session nothing is happening in as idle', () => {
    expect(sessionStatus({ session_id: 's1' })).toBe('idle');
  });

  it('reads a streaming session as working', () => {
    attentionActions.report('s1', { isStreaming: true });
    expect(sessionStatus({ session_id: 's1' })).toBe('working');
  });

  it('reads a blocked session as waiting', () => {
    attentionActions.report('s1', { pendingInteraction: ASK });
    expect(sessionStatus({ session_id: 's1' })).toBe('waiting');
  });

  it('calls a session that streams AND blocks waiting — the human is the blocker', () => {
    attentionActions.report('s1', { isStreaming: true, pendingInteraction: ASK });
    expect(sessionStatus({ session_id: 's1' })).toBe('waiting');
  });
});

describe('STATUS_RANK', () => {
  it('puts what blocks you above what merely runs', () => {
    const order = (['idle', 'working', 'waiting'] as const)
      .slice()
      .sort((a, b) => STATUS_RANK[a] - STATUS_RANK[b]);
    expect(order).toEqual(['waiting', 'working', 'idle']);
  });
});
