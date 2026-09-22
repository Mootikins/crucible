/**
 * The diffset surface, over the axum bridge.
 *
 * The web server builds each source from the query: `root` for a branch,
 * `session` for a session record, `proposal` for a proposal.
 */
import { client, decode } from './api-client';
import {
  unreachable,
  type DiffComment,
  type DiffFileEntry,
  type DiffFileText,
  type Diffset,
  type DiffsetSource,
  type ListedComment,
  type NewDiffComment,
} from './diffset';

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

/** The proposal fields of a query. A proposal takes no base and no head. */
function proposalQuery(source: Extract<DiffsetSource, { kind: 'proposal' }>) {
  return { proposal: source.id };
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
      return decode(
        await client.GET('/api/diff', { params: { query: proposalQuery(source) } }),
        'Failed to load the diff',
      );
    default:
      return unreachable(source);
  }
}

/**
 * The two texts of one file. A side is null when the file is absent on it,
 * binary or too large.
 *
 * A session record and a proposal can span more than one root. The request
 * therefore sends the root of the entry, and the daemon reads the file below
 * that root.
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
      return decode(
        await client.GET('/api/diff/file', {
          params: {
            query: { ...proposalQuery(source), root: entry.root, path: entry.path, ...from },
          },
        }),
        `Failed to load ${entry.path}`,
      );
    default:
      return unreachable(source);
  }
}

/**
 * The comments of one diffset, oldest first. The daemon marks each comment
 * whose quoted text is gone as outdated.
 */
export async function getDiffComments(source: DiffsetSource): Promise<ListedComment[]> {
  let query;
  switch (source.kind) {
    case 'branch':
      query = branchQuery(source);
      break;
    case 'session_record':
      query = sessionQuery(source);
      break;
    case 'proposal':
      query = proposalQuery(source);
      break;
    default:
      return unreachable(source);
  }
  const reply = decode(
    await client.GET('/api/diff/comments', { params: { query } }),
    'Failed to load the comments',
  );
  return reply.comments;
}

/** Stores one comment on a line range of one file. The reply is the stored comment. */
export async function postDiffComment(body: NewDiffComment): Promise<DiffComment> {
  const reply = decode(
    await client.POST('/api/diff/comment', { body }),
    'Failed to save the comment',
  );
  return reply.comment;
}

/**
 * Marks one comment of the diffset of `source` resolved. The reply names the
 * diffset and the comment.
 */
export async function resolveDiffComment(
  source: DiffsetSource,
  commentId: string,
): Promise<{ diffset: string; comment_id: string; resolved: boolean }> {
  return decode(
    await client.POST('/api/diff/comment/resolve', {
      body: { source, comment_id: commentId },
    }),
    'Failed to resolve the comment',
  );
}
