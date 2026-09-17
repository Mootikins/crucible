/**
 * The stream protocol version contract (Task G6).
 *
 * Every server-sent-event stream names the protocol it speaks, twice: the
 * `X-Crucible-Stream-Version` response header, and — because the browser's
 * own `EventSource` cannot read response headers at all — the stream's first
 * `stream_version` frame, carrying `{"version": 1}`.
 *
 * A client that meets a version it does not understand fails closed: it
 * closes the source and raises, rendering nothing the stream carries. Daemon
 * and browser ship from one repo so skew is small, but a mis-parsed stream is
 * worse than a refused one — a half-read transcript looks like the truth.
 */

/** The stream protocol version this build understands. */
export const STREAM_VERSION = 1;

/** Raised when a stream answers a protocol version this build cannot read. */
export class StreamVersionError extends Error {
  constructor(stream: string, version: unknown) {
    super(
      `the ${stream} stream speaks protocol version ${String(version)}; ` +
        `this build understands only ${STREAM_VERSION}. Nothing from it was rendered.`,
    );
    this.name = 'StreamVersionError';
  }
}

/**
 * Reads one `stream_version` handshake frame and refuses the stream when the
 * version is not one this build speaks.
 *
 * An ABSENT handshake is the legacy protocol this build has always read, and
 * is allowed: every stream predating the version contract (and every test
 * double standing in for one) sends no handshake at all.
 */
export function assertStreamVersion(stream: string, raw: string): void {
  let version: unknown;
  try {
    version = JSON.parse(raw).version;
  } catch {
    throw new StreamVersionError(stream, 'unreadable');
  }
  if (version !== STREAM_VERSION) {
    throw new StreamVersionError(stream, version);
  }
}
