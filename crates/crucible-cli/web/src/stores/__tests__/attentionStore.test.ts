import { describe, it, expect, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { attentionStore, attentionActions } from '../attentionStore';
import type { PermRequest } from '@/lib/types';

const perm: PermRequest = {
  kind: 'permission',
  id: 'req-1',
  action_type: 'bash',
  tokens: ['cargo', 'test'],
  tool_name: 'Bash',
};

beforeEach(() => {
  for (const id of Object.keys(attentionStore.entries)) {
    attentionActions.clear(id);
  }
});

describe('attentionStore', () => {
  it('starts empty with a zero badge', () => {
    createRoot((dispose) => {
      expect(attentionStore.attentionCount()).toBe(0);
      expect(attentionStore.waiting()).toEqual([]);
      dispose();
    });
  });

  it('counts sessions with a pending interaction', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm, title: 'scheduler' });
      attentionActions.report('s2', { isStreaming: true, title: 'other' });

      expect(attentionStore.attentionCount()).toBe(1);
      expect(attentionStore.waiting()[0].sessionId).toBe('s1');
      expect(attentionStore.streamingCount()).toBe(1);
      dispose();
    });
  });

  it('drops the badge when the interaction resolves', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm });
      expect(attentionStore.attentionCount()).toBe(1);

      attentionActions.report('s1', { pendingInteraction: null });
      expect(attentionStore.attentionCount()).toBe(0);
      dispose();
    });
  });

  it('merges patches without losing earlier state', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm, title: 'scheduler' });
      attentionActions.report('s1', { isStreaming: true });

      const entry = attentionStore.get('s1');
      expect(entry?.pendingInteraction).toEqual(perm);
      expect(entry?.isStreaming).toBe(true);
      expect(entry?.title).toBe('scheduler');
      dispose();
    });
  });

  it('clear removes the session entirely', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm });
      attentionActions.clear('s1');
      expect(attentionStore.get('s1')).toBeUndefined();
      expect(attentionStore.attentionCount()).toBe(0);
      dispose();
    });
  });
});
