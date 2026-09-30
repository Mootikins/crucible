/**
 * The diffset surface, over `POST /api/rpc/{method}`.
 *
 * `DiffsetSource` names a branch, a session record or a proposal, the same
 * tagged union `diff.get`/`diff.file`/`diff.comments` take as their own
 * `source` field — so a caller here builds the row's params directly, with
 * no flat-query reshaping in between (Simplification Plan step 19: the
 * reshaping used to exist only because `GET /api/diff` read a query string,
 * which `rpc()`'s JSON body does not need).
 */
import { rpc } from './api-client';
import type {
  DiffComment,
  DiffFileEntry,
  DiffFileText,
  Diffset,
  DiffsetSource,
  ListedComment,
  NewDiffComment,
} from './diffset';

/** The files of one diffset, with their counts and no text. */
export async function getDiffset(source: DiffsetSource): Promise<Diffset> {
  return rpc('diff.get', { source });
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
  const from = entry.status.kind === 'renamed' ? entry.status.from : undefined;
  return rpc('diff.file', {
    source,
    path: entry.path,
    from,
    root: source.kind === 'branch' ? undefined : entry.root,
  });
}

/**
 * The comments of one diffset, oldest first. The daemon marks each comment
 * whose quoted text is gone as outdated.
 */
export async function getDiffComments(source: DiffsetSource): Promise<ListedComment[]> {
  return (await rpc('diff.comments', { source })).comments;
}

/** Stores one comment on a line range of one file. The reply is the stored comment. */
export async function postDiffComment(body: NewDiffComment): Promise<DiffComment> {
  return (await rpc('diff.comment', body)).comment;
}

/**
 * Marks one comment of the diffset of `source` resolved. The reply names the
 * diffset and the comment.
 */
export async function resolveDiffComment(
  source: DiffsetSource,
  commentId: string,
): Promise<{ diffset: string; comment_id: string; resolved: boolean }> {
  return rpc('diff.resolve_comment', { source, comment_id: commentId });
}

/**
 * Removes one comment of the diffset of `source` from the store.
 *
 * Delete is not resolve. Resolve keeps a settled remark in the record; delete
 * says that the author never wrote the remark, so the comment leaves the
 * diff pane and the quickfix list with it.
 */
export async function deleteDiffComment(
  source: DiffsetSource,
  commentId: string,
): Promise<{ diffset: string; comment_id: string; deleted: boolean }> {
  return rpc('diff.delete_comment', { source, comment_id: commentId });
}
