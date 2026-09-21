/**
 * The diffset surface, over the axum bridge.
 *
 * The web server builds the branch source and the session record source. The
 * proposal source refuses here with a sentence, not with a request that the
 * server cannot parse.
 */
import { client, decode } from './api-client';
import {
  unreachable,
  type DiffFileEntry,
  type DiffFileText,
  type Diffset,
  type DiffsetSource,
} from './diffset';

/** The error of a source that the web server does not serve yet. */
function notServed(source: DiffsetSource): Error {
  return new Error(`The web server does not serve the ${source.kind} diffset yet`);
}

/** The branch fields of a query. Absent fields take the daemon's default. */
function branchQuery(source: Extract<DiffsetSource, { kind: 'branch' }>) {
  return {
    root: source.root,
    ...(source.base ? { base: source.base } : {}),
    ...(source.head != null ? { head: source.head } : {}),
  };
}

/** The session fields of a query. A session record takes no base and no head. */
function sessionQuery(source: Extract<DiffsetSource, { kind: 'session_record' }>) {
  return { session: source.session };
}

/** The files of one diffset, with their counts and no text. */
export async function getDiffset(source: DiffsetSource): Promise<Diffset> {
  switch (source.kind) {
    case 'branch':
      return decode(
        await client.GET('/api/diff', { params: { query: branchQuery(source) } }),
        'Failed to load the diff',
      );
    case 'session_record':
      return decode(
        await client.GET('/api/diff', { params: { query: sessionQuery(source) } }),
        'Failed to load the diff',
      );
    case 'proposal':
      throw notServed(source);
    default:
      return unreachable(source);
  }
}

/**
 * The two texts of one file. A side is null when the file is absent on it,
 * binary or too large.
 *
 * A session record can span more than one root. The request therefore sends
 * the root of the entry, and the daemon reads the file below that root.
 */
export async function getDiffFile(
  source: DiffsetSource,
  entry: Pick<DiffFileEntry, 'root' | 'path' | 'status'>,
): Promise<DiffFileText> {
  const from = entry.status.kind === 'renamed' ? { from: entry.status.from } : {};
  switch (source.kind) {
    case 'branch':
      return decode(
        await client.GET('/api/diff/file', {
          params: { query: { ...branchQuery(source), path: entry.path, ...from } },
        }),
        `Failed to load ${entry.path}`,
      );
    case 'session_record':
      return decode(
        await client.GET('/api/diff/file', {
          params: {
            query: { ...sessionQuery(source), root: entry.root, path: entry.path, ...from },
          },
        }),
        `Failed to load ${entry.path}`,
      );
    case 'proposal':
      throw notServed(source);
    default:
      return unreachable(source);
  }
}
