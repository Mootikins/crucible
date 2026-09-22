import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  diffsetKey,
  type DiffComment,
  type DiffFileEntry,
  type DiffFileText,
  type Diffset,
  type DiffsetSource,
  type ListedComment,
  type NewDiffComment,
} from '@/lib/diffset';
import {
  deleteDiffComment,
  getDiffComments,
  getDiffFile,
  getDiffset,
  postDiffComment,
  resolveDiffComment,
} from '@/lib/diff-api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * One diffset, and the texts of its files.
 *
 * The list and the texts are separate entries. The list has only the counts,
 * and `DiffPanel` asks for the text of a file when the user expands it. A
 * branch with 400 files therefore does not send 800 texts at once.
 */

/** The files of one diffset, with their counts and no text. */
export function useDiffset(source: Accessor<DiffsetSource>): UseQueryResult<Diffset, Error> {
  return useQuery(() => {
    const value = source();
    return {
      queryKey: keys.diffset(diffsetKey(value)),
      queryFn: () => getDiffset(value),
    };
  }, getQueryClient);
}

/** The two texts of one file of a diffset. */
export function useDiffFile(
  source: Accessor<DiffsetSource>,
  entry: Accessor<DiffFileEntry>,
): UseQueryResult<DiffFileText, Error> {
  return useQuery(() => {
    const value = source();
    const file = entry();
    const from = file.status.kind === 'renamed' ? file.status.from : undefined;
    return {
      queryKey: keys.diffFile(diffsetKey(value), file.root, file.path, from),
      queryFn: () => getDiffFile(value, file),
    };
  }, getQueryClient);
}

/** The comments of one diffset, each with its outdated flag. */
export function useDiffComments(
  source: Accessor<DiffsetSource>,
): UseQueryResult<ListedComment[], Error> {
  return useQuery(() => {
    const value = source();
    return {
      queryKey: keys.diffComments(diffsetKey(value)),
      queryFn: () => getDiffComments(value),
    };
  }, getQueryClient);
}

/** Stores one comment. The comments of its diffset then load again. */
export function usePostDiffComment(): UseMutationResult<DiffComment, Error, NewDiffComment> {
  return useMutation(
    () => ({
      mutationFn: postDiffComment,
      onSuccess: (_stored, body) =>
        getQueryClient().invalidateQueries({
          queryKey: keys.diffComments(diffsetKey(body.source)),
        }),
    }),
    getQueryClient,
  );
}

/** One comment, named by its id and by the diffset that holds it. */
export interface DiffCommentRef {
  source: DiffsetSource;
  commentId: string;
}

/**
 * Marks one comment resolved. The comments of its diffset then load again, and
 * a session record also loads the listing of its Changes panel again.
 */
export function useResolveDiffComment(): UseMutationResult<
  { comment_id: string },
  Error,
  DiffCommentRef
> {
  return useMutation(
    () => ({
      mutationFn: ({ source, commentId }: DiffCommentRef) => resolveDiffComment(source, commentId),
      onSettled: (_reply, _error, { source }) =>
        Promise.all([
          getQueryClient().invalidateQueries({ queryKey: keys.diffComments(diffsetKey(source)) }),
          source.kind === 'session_record'
            ? getQueryClient().invalidateQueries({ queryKey: keys.review(source.session) })
            : undefined,
        ]),
    }),
    getQueryClient,
  );
}

/**
 * Removes one comment. The comments of its diffset then load again, and a
 * session record also loads the listing of its Changes panel again.
 *
 * The chip in the composer and the comment in the pane are one thing: the
 * `×` of the chip calls this, and the comment leaves the pane.
 */
export function useDeleteDiffComment(): UseMutationResult<
  { comment_id: string },
  Error,
  DiffCommentRef
> {
  return useMutation(
    () => ({
      mutationFn: ({ source, commentId }: DiffCommentRef) => deleteDiffComment(source, commentId),
      onSettled: (_reply, _error, { source }) =>
        Promise.all([
          getQueryClient().invalidateQueries({ queryKey: keys.diffComments(diffsetKey(source)) }),
          source.kind === 'session_record'
            ? getQueryClient().invalidateQueries({ queryKey: keys.review(source.session) })
            : undefined,
        ]),
    }),
    getQueryClient,
  );
}

/** Makes one diffset and the texts of its files wrong. Refresh calls it. */
export function invalidateDiffset(source: DiffsetSource): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: keys.diffset(diffsetKey(source)) })
    .then(() => undefined);
}
