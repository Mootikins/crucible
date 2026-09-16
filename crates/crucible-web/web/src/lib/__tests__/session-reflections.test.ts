import { describe, it, expect } from 'vitest';
import { reflectionSessions } from '@/lib/session-reflections';
import type { Session } from '@/lib/types';

const at = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();

const session = (over: Partial<Session>): Session =>
  ({
    id: 'x',
    session_type: 'chat',
    kilns: [],
    workspace: null,
    state: 'active',
    title: null,
    archived: false,
    started_at: at(10),
    last_activity: at(10),
    event_count: 0,
    ...over,
  }) as Session;

describe('reflectionSessions', () => {
  it('keeps only the sessions a plugin started', () => {
    const list = [
      session({ session_id: 'chat', type: 'chat' }),
      session({ session_id: 'pass', type: 'plugin' }),
      session({ session_id: 'flow', type: 'workflow' }),
    ];
    expect(reflectionSessions(list).map((s) => s.session_id)).toEqual(['pass']);
  });

  it('leaves out an archived pass', () => {
    const list = [session({ session_id: 'old', type: 'plugin', archived: true })];
    expect(reflectionSessions(list)).toEqual([]);
  });

  it('puts the freshest first', () => {
    const list = [
      session({ session_id: 'older', type: 'plugin', last_activity: at(60) }),
      session({ session_id: 'newer', type: 'plugin', last_activity: at(1) }),
    ];
    expect(reflectionSessions(list).map((s) => s.session_id)).toEqual(['newer', 'older']);
  });
});
