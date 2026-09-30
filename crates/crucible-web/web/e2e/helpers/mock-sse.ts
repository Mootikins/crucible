import type { Page } from '@playwright/test';

/** The stream protocol version the web client accepts (see `lib/stream-version.ts`). */
const STREAM_VERSION = 1;

/** One frame of a mocked stream, before `createSSEStream` writes it. */
export type SseFrame = { type: string; data: object };

/**
 * The frames of a transcript, in the order the daemon sends them.
 *
 * The daemon folds each event and sends the ops of the fold in a `transcript`
 * frame (`crates/crucible-core/src/transcript/`). A spec states the ops, and
 * the web client applies them; the spec never folds events itself. Each
 * frame takes the next seq. The items come from `src/test-utils/transcript.ts`.
 */
export class TranscriptFrames {
  private seq = 0;
  /** The UTF-8 length of the text of each segment, for the `at` of an append. */
  private readonly lengths = new Map<string, number>();

  /** A frame of `ops`. */
  ops(ops: object[]): SseFrame {
    this.seq += 1;
    return { type: 'transcript', data: { type: 'transcript', seq: this.seq, ops } };
  }

  /** Adds `item`, or replaces the item with its id. */
  upsert(item: { id: string; type: string; text?: unknown }): SseFrame {
    if (item.type === 'assistant_segment') {
      this.lengths.set(item.id, new TextEncoder().encode(String(item.text ?? '')).length);
    }
    return this.ops([{ op: 'upsert', item }]);
  }

  /** One append frame for each chunk of `text`, onto the text of the segment `id`. */
  appends(id: string, text: string, chunk: number): SseFrame[] {
    const frames: SseFrame[] = [];
    let at = this.lengths.get(id) ?? 0;
    for (let i = 0; i < text.length; i += chunk) {
      const part = text.slice(i, i + chunk);
      frames.push(this.ops([{ op: 'append', id, field: 'text', at, text: part }]));
      at += new TextEncoder().encode(part).length;
    }
    this.lengths.set(id, at);
    return frames;
  }
}

/** Serialize events to SSE wire format */
export function createSSEStream(events: Array<{ type: string; data: object }>): string {
  const handshake = `event: stream_version\ndata: ${JSON.stringify({ version: STREAM_VERSION })}\n\n`;
  const body = events.map((e) => `event: ${e.type}\ndata: ${JSON.stringify(e.data)}\n\n`).join('');
  return handshake + body;
}

/** Headers every mocked SSE response must carry (G6). */
export const SSE_HEADERS = {
  'Content-Type': 'text/event-stream',
  'Cache-Control': 'no-cache',
  Connection: 'keep-alive',
  'X-Crucible-Stream-Version': String(STREAM_VERSION),
} as const;

/**
 * Mocks the one shared event stream (`GET /api/events`, Simplification Plan
 * step 19), which replaced the separate chat, filesystem, surface and system
 * mocks this helper used to register one `page.route` per stream for.
 *
 * A request names every topic it wants in `?topics=a,b,...`; this answers
 * `sessionFrames` for every topic that is not `system` (a chat session's own
 * topic, whatever its id — the mocks never cared which one) and nothing for
 * `system` (the filesystem watcher, the surface changes and the daemon's
 * publications and proposals, none of which a spec here fabricates). Each
 * answered frame's body gains the topic it travelled on, so the app's own
 * dispatcher — which reads that field to route a frame at all — delivers it.
 */
export async function mockEventsRoute(
  page: Page,
  sessionFrames: Array<{ type: string; data: object }>,
): Promise<void> {
  await page.route('**/api/events?**', (route) => {
    const requested = new URL(route.request().url()).searchParams.get('topics')?.split(',') ?? [];
    const frames = requested.flatMap((topic) =>
      topic === 'system'
        ? []
        : sessionFrames.map((frame) => ({ type: frame.type, data: { ...frame.data, topic } })),
    );
    route.fulfill({
      status: 200,
      headers: SSE_HEADERS,
      body: createSSEStream(frames),
    });
  });
}
