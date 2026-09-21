/**
 * The diffset values of the web client, without any request.
 *
 * `DiffsetSource` is a closed set. The daemon has one exhaustive match on it.
 * The switches here and in `diff-api.ts` are the matches of the web client,
 * and `unreachable` fails the compile when a variant has no branch.
 *
 * This module imports no API client, so the tab actions can use it.
 */
import type { components } from './api-schema';

type Schemas = components['schemas'];

export type Diffset = Schemas['Diffset'];
export type DiffsetSource = Schemas['DiffsetSource'];
export type DiffFileEntry = Schemas['DiffFileEntry'];
export type DiffFileText = Schemas['DiffFileText'];

/** Fails the compile when a new source variant has no branch here. */
export function unreachable(source: never): never {
  throw new Error(`Unknown diffset source: ${JSON.stringify(source)}`);
}

/**
 * The key of one diffset in the client: the tab key and the cache key.
 *
 * The daemon hashes a branch source with BLAKE3, and the client cannot compute
 * that hash. The client therefore writes the fields out. A git ref contains no
 * ":" and no "..", so the key is not ambiguous. The other two keys are equal to
 * the daemon's id.
 */
export function diffsetKey(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `branch:${source.root}:${source.base}..${source.head ?? ''}`;
    case 'session_record':
      return `session-${source.session}`;
    case 'proposal':
      return `proposal-${source.id}`;
    default:
      return unreachable(source);
  }
}

/** The last segment of a path. */
function basename(path: string): string {
  return path.replace(/\/+$/, '').split('/').pop() || path;
}

/** The title of the tab of one diffset. */
export function diffsetTitle(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `Diff: ${basename(source.root)}`;
    case 'session_record':
      return 'Diff: session';
    case 'proposal':
      return 'Diff: proposal';
    default:
      return unreachable(source);
  }
}

/**
 * The header text of one diffset, for example "Branch working tree → master".
 * An empty base is the default branch, which the reply of the daemon names.
 */
export function diffsetLabel(source: DiffsetSource): string {
  switch (source.kind) {
    case 'branch':
      return `Branch ${source.head ?? 'working tree'} → ${source.base || 'default branch'}`;
    case 'session_record':
      return `Session ${source.session}`;
    case 'proposal':
      return `Proposal ${source.id}`;
    default:
      return unreachable(source);
  }
}
