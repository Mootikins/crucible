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
      session({ id: 'chat', session_type: 'chat' }),
      session({ id: 'pass', session_type: 'plugin' }),
      session({ id: 'flow', session_type: 'workflow' }),
    ];
    expect(reflectionSessions(list).map((s) => s.id)).toEqual(['pass']);
  });

  it('leaves out an archived pass', () => {
    const list = [session({ id: 'old', session_type: 'plugin', archived: true })];
    expect(reflectionSessions(list)).toEqual([]);
  });

  it('puts the freshest first', () => {
    const list = [
      session({ id: 'older', session_type: 'plugin', last_activity: at(60) }),
      session({ id: 'newer', session_type: 'plugin', last_activity: at(1) }),
    ];
    expect(reflectionSessions(list).map((s) => s.id)).toEqual(['newer', 'older']);
  });
});
