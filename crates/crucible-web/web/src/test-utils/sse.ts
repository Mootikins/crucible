import { expect, vi } from 'vitest';

/**
 * The `EventSource` of a test.
 *
 * jsdom has none, so every test of a server-sent-event stream installs this
 * one over the global and drives it by hand: `open()` and `emit()` act as the
 * server, `closed` proves that the last subscriber closed the source.
 *
 * It lives here, and not inside one spec, because `lib/query/sse.ts` holds one
 * root per stream and each entity task of Part C and D tests its own route
 * against the same four streams.
 */
export class FakeEventSource {
  /** Every source built since the last `installFakeEventSource`, in order. */
  static instances: FakeEventSource[] = [];

  readonly listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  closed = false;
  onopen: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void): void {
    let perType = this.listeners.get(type);
    if (!perType) this.listeners.set(type, (perType = new Set()));
    perType.add(listener);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void): void {
    this.listeners.get(type)?.delete(listener);
  }

  close(): void {
    this.closed = true;
  }

  /** Acts as the server: the stream is open. */
  open(): void {
    this.onopen?.(new Event('open'));
  }

  /** Acts as the server: one frame of the named type arrives. */
  emit(type: string, data: unknown, options?: { lastEventId?: string }): void {
    // A closed source delivers nothing — the browser's own behavior, and the
    // fail-closed version gate depends on it.
    if (this.closed) return;
    for (const listener of [...(this.listeners.get(type) ?? [])]) {
      listener({
        data: JSON.stringify(data),
        lastEventId: options?.lastEventId ?? '',
      } as MessageEvent);
    }
  }
}

/**
 * Puts `FakeEventSource` over the global `EventSource`, and forgets the sources
 * of the test before.
 *
 * Vitest undoes the global itself (`unstubGlobals`), so a test needs no
 * removal of its own.
 */
export function installFakeEventSource(): typeof FakeEventSource {
  FakeEventSource.instances = [];
  vi.stubGlobal('EventSource', FakeEventSource);
  return FakeEventSource;
}

/**
 * The one source the test expects.
 *
 * It fails with the count when there are more, because two sources for one
 * stream is the fault these tests exist to find.
 */
export function onlyEventSource(): FakeEventSource {
  expect(FakeEventSource.instances).toHaveLength(1);
  return FakeEventSource.instances[0]!;
}
