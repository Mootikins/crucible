import { describe, it, expect, beforeEach } from 'vitest';
import { installFakeEventSource, FakeEventSource } from '@/test-utils/sse';
import { subscribeToEvents } from '@/lib/api';
import { STREAM_VERSION, assertStreamVersion, StreamVersionError } from '@/lib/stream-version';

// No `vi.mock('@/lib/api')`. The gate runs inside the REAL subscribeToEvents
// against the FakeEventSource, so what these cases prove is the transport's
// own behavior: a version this build cannot read closes the source and
// renders nothing, before any payload frame is delivered.

beforeEach(() => {
  installFakeEventSource();
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
    source.emit('token', { type: 'token', content: 'should not render' });
    source.emit('message_complete', { type: 'message_complete', id: 'm1', content: 'nor this' });

    expect(seen).toEqual([]);
    expect(source.closed).toBe(true);
  });

  it('the version this build speaks lets the stream through', () => {
    const seen: unknown[] = [];
    subscribeToEvents('s1', (event) => seen.push(event));

    const source = FakeEventSource.instances[0]!;
    source.emit('stream_version', { version: STREAM_VERSION });
    source.emit('token', { type: 'token', content: 'renders' });

    expect(seen).toEqual([{ type: 'token', content: 'renders' }]);
    expect(source.closed).toBe(false);
  });

  it('a stream with no handshake is the legacy protocol and is allowed', () => {
    const seen: unknown[] = [];
    subscribeToEvents('s1', (event) => seen.push(event));

    const source = FakeEventSource.instances[0]!;
    source.emit('token', { type: 'token', content: 'legacy' });

    expect(seen).toEqual([{ type: 'token', content: 'legacy' }]);
    expect(source.closed).toBe(false);
  });
});
