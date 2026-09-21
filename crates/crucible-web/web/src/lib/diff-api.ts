/**
 * The diffset surface, over the axum bridge.
 *
 * Today the web server builds only the branch source. The other two variants
 * refuse here with a sentence, not with a request the server cannot parse.
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

/** The files of one diffset, with their counts and no text. */
export async function getDiffset(source: DiffsetSource): Promise<Diffset> {
  switch (source.kind) {
    case 'branch':
      return decode(
        await client.GET('/api/diff', { params: { query: branchQuery(source) } }),
        'Failed to load the diff',
      );
    case 'session_record':
    case 'proposal':
      throw notServed(source);
    default:
      return unreachable(source);
  }
}

/**
 * The two texts of one file. A side is null when the file is absent on it,
 * binary or too large.
 */
export async function getDiffFile(
  source: DiffsetSource,
  entry: Pick<DiffFileEntry, 'path' | 'status'>,
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
    case 'proposal':
      throw notServed(source);
    default:
      return unreachable(source);
  }
}
