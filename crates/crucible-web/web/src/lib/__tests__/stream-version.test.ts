import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { installFakeEventSource, FakeEventSource } from '@/test-utils/sse';
import { resetEventsConnectionForTests, subscribeToEvents } from '@/lib/api';
import { STREAM_VERSION, assertStreamVersion, StreamVersionError } from '@/lib/stream-version';

// No `vi.mock('@/lib/api')`. The gate runs inside the REAL subscribeToEvents
// against the FakeEventSource, so what these cases prove is the transport's
// own behavior: a version this build cannot read closes the source and
// renders nothing, before any payload frame is delivered.

beforeEach(() => {
  installFakeEventSource();
});

afterEach(() => {
  // `subscribeToEvents` joins the module-level shared connection directly
  // here (not through a `lib/query/sse.ts` root), so each case must leave
  // its topic itself or the next case's `subscribeToEvents('s1', ...)` finds
  // `s1` already joined and opens no fresh source to assert against.
  resetEventsConnectionForTests();
});

describe('assertStreamVersion', () => {
  it('accepts the version this build speaks', () => {
    expect(() => assertStreamVersion('chat', `{"version":${STREAM_VERSION}}`)).not.toThrow();
  });

  it('raises on a newer version', () => {
    expect(() => assertStreamVersion('chat', '{"version":2}')).toThrowError(StreamVersionError);
  });

  it('raises on a frame it cannot read', () => {
    expect(() => assertStreamVersion('chat', 'not json')).toThrowError(StreamVersionError);
  });
});

describe('the stream version gate in subscribeToEvents', () => {
  it('a 2 answer closes the source and renders nothing', () => {
    const seen: unknown[] = [];
    subscribeToEvents('s1', (event) => seen.push(event));

    const source = FakeEventSource.instances[0]!;
    source.emit('stream_version', { version: 2 });
    // Payload frames after the refusal must not reach the handler — a
    // half-read transcript looks like the truth, which is worse than none.
    source.emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'should not render' } });
    source.emit('message_complete', {
      topic: 's1',
      event: 'message_complete',
      data: { message_id: 'm1', full_response: 'nor this' },
    });

    // The shared connection now reports every refusal through `onDisconnect`
    // (Simplification Plan step 19 harmonised chat with the other three
    // domains, which always did), so a synthetic `connection` event is the
    // one thing `seen` carries — never a payload frame.
    expect(seen).toEqual([
      { type: 'connection', status: 'reconnecting', message: 'Reconnecting…' },
    ]);
    expect(source.closed).toBe(true);
  });

  it('the version this build speaks lets the stream through', () => {
    const seen: unknown[] = [];
    subscribeToEvents('s1', (event) => seen.push(event));

    const source = FakeEventSource.instances[0]!;
    source.emit('stream_version', { version: STREAM_VERSION });
    source.emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'renders' } });

    expect(seen).toEqual([{ event: 'text_delta', data: { content: 'renders' } }]);
    expect(source.closed).toBe(false);
  });

  it('a stream with no handshake is the legacy protocol and is allowed', () => {
    const seen: unknown[] = [];
    subscribeToEvents('s1', (event) => seen.push(event));

    const source = FakeEventSource.instances[0]!;
    source.emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'legacy' } });

    expect(seen).toEqual([{ event: 'text_delta', data: { content: 'legacy' } }]);
    expect(source.closed).toBe(false);
  });
});
