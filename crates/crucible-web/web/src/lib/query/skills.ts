import type { Accessor } from 'solid-js';
import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import { rpc } from '@/lib/api-client';
import type { SkillSummary } from '@/lib/types';
import type { RpcMethods } from '@/lib/rpc-methods';
import { getQueryClient } from './client';
import { keys } from './keys';

/** One skill's full body and metadata: the `skills.get` row's own reply. */
export type SkillDetail = RpcMethods['skills.get']['result'];

/**
 * The skills of one kiln: the roster, the search over it, and one skill's body.
 *
 * All three arguments arrive as accessors because the panel learns them late
 * and changes them often. The kiln is the clearest case: the panel mounts
 * before it knows one, because the session names a kiln and that name has to
 * be resolved to a directory first. A hook that took a plain string would ask
 * the daemon to list the skills of nowhere, which is the request the panel's
 * old resource guarded against with an `if (!kiln) return []`.
 *
 * The kiln is IN each key, not a filter applied after the fetch. A panel on one
 * kiln and a panel on another hold two entries, and neither draws the other's
 * skills.
 */

/** Nothing to ask about: no kiln resolved yet. */
function withKiln(kiln: string | null | undefined): kiln is string {
  return typeof kiln === 'string' && kiln.length > 0;
}

/** Every skill discovered for one kiln. */
export function useSkillList(
  kiln: Accessor<string | null | undefined>,
): UseQueryResult<SkillSummary[], Error> {
  return useQuery(
    () => ({
      queryKey: keys.skillsList(kiln() ?? ''),
      queryFn: async () =>
        (await rpc('skills.list', { kiln_path: kiln() as string })).skills,
      enabled: withKiln(kiln()),
    }),
    getQueryClient,
  );
}

/**
 * The daemon's own search over one kiln's skills.
 *
 * A blank query is not a search: the daemon would match every skill, which is
 * the roster the panel already holds. Whitespace counts as blank, because a
 * user who clears the box leaves one behind often enough.
 *
 * The caller debounces. The key holds the debounced text, so a query typed once
 * and typed again answers from the cache rather than from the daemon.
 */
export function useSkillSearch(
  kiln: Accessor<string | null | undefined>,
  query: Accessor<string>,
): UseQueryResult<SkillSummary[], Error> {
  const asked = (): string => query().trim();
  return useQuery(
    () => ({
      queryKey: keys.skillsSearch(kiln() ?? '', asked()),
      queryFn: async () =>
        (await rpc('skills.search', { query: asked(), kiln_path: kiln() as string })).skills,
      enabled: withKiln(kiln()) && asked().length > 0,
    }),
    getQueryClient,
  );
}

/**
 * One skill's full body and metadata.
 *
 * `name` is null while the drawer is closed. Each skill is its own entry, so
 * re-opening one the user read a moment ago draws it without a second read of
 * the file.
 */
export function useSkillDetail(
  name: Accessor<string | null | undefined>,
  kiln: Accessor<string | null | undefined>,
): UseQueryResult<SkillDetail, Error> {
  return useQuery(
    () => ({
      queryKey: keys.skillDetail(name() ?? '', kiln() ?? ''),
      queryFn: () => rpc('skills.get', { name: name() as string, kiln_path: kiln() as string }),
      enabled: withKiln(kiln()) && !!name(),
    }),
    getQueryClient,
  );
}
