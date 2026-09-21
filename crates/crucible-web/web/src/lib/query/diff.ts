import type { Accessor } from 'solid-js';
import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import {
  diffsetKey,
  type DiffFileEntry,
  type DiffFileText,
  type Diffset,
  type DiffsetSource,
} from '@/lib/diffset';
import { getDiffFile, getDiffset } from '@/lib/diff-api';
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
      queryKey: keys.diffFile(diffsetKey(value), file.path, from),
      queryFn: () => getDiffFile(value, file),
    };
  }, getQueryClient);
}

/** Makes one diffset and the texts of its files wrong. Refresh calls it. */
export function invalidateDiffset(source: DiffsetSource): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: keys.diffset(diffsetKey(source)) })
    .then(() => undefined);
}
