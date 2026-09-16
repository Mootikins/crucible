import { createEffect, createSignal, on, onCleanup, type Accessor } from 'solid-js';
import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import {
  grepSearch,
  searchSessions,
  semanticSearch,
  type GrepResponse,
  type SemanticHit,
} from '@/lib/api';
import type { Session } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The three searches, each held under the question it answers.
 *
 * A search is a READ like any other, and holding it means the two things the
 * panel used to do by hand stop being its problem. It debounced the typing
 * itself, and it carried a `runToken` so a slow answer for a query the user
 * had already moved past did not overwrite a fast answer for the current one.
 * A keyed entry gives the second for free: an answer belongs to its question,
 * and an answer for a question nobody is asking any more reaches no reader.
 *
 * Going back one character is then free as well, which it was not: the panel
 * re-ran every search from the top for a query it had just shown.
 */

/**
 * How long the panel waits after a keystroke before it asks.
 *
 * Grep walks a tree and a semantic search embeds the text with a provider, so
 * one request per character is one walk per character. It lives here beside
 * the reads it throttles, and both hooks use it, so the three searches of one
 * keystroke still go out together.
 */
export const SEARCH_DEBOUNCE_MS = 220;

/**
 * How long an answer stays fresh.
 *
 * Deliberately far shorter than the app-wide window. A search describes files
 * that are being edited while the panel is open, so a five-minute-old answer
 * is a list of matches that have moved; half a minute is long enough for the
 * back-and-forth of narrowing a query and short enough that reopening the
 * panel later asks again.
 */
const SEARCH_STALE_MS = 30_000;

/** How many hits the daemon returns. NOT in the key — see `grepOptions`. */
const HIT_LIMIT = 60;
const SEMANTIC_LIMIT = 20;
const SESSION_LIMIT = 30;

/**
 * The query text, a moment after the typing stops.
 *
 * An EMPTY query clears at once rather than after the wait: clearing the box
 * is not a search, and making the user watch the old hits for a fifth of a
 * second after it says the panel did not notice.
 */
function useDebouncedQuery(query: Accessor<string>): Accessor<string> {
  const [held, setHeld] = createSignal(query().trim());
  createEffect(
    on(query, (text) => {
      const trimmed = text.trim();
      if (!trimmed) {
        setHeld('');
        return;
      }
      const timer = setTimeout(() => setHeld(trimmed), SEARCH_DEBOUNCE_MS);
      onCleanup(() => clearTimeout(timer));
    }),
  );
  return held;
}

/**
 * The options of one grep.
 *
 * The limit is fixed here rather than taken from the caller, because it is not
 * in the key: two callers asking for different depths of the same search would
 * share one entry and one of them would be served the other's shorter list.
 */
function grepOptions(root: string, query: string, glob?: string) {
  return {
    queryKey: keys.searchGrep(root, query, glob),
    queryFn: () => grepSearch(root, query, { glob, limit: HIT_LIMIT }),
    staleTime: SEARCH_STALE_MS,
  };
}

/** Notes ranked by vector similarity, over one kiln. */
export function useSemanticSearch(
  kiln: Accessor<string | null>,
  query: Accessor<string>,
): UseQueryResult<SemanticHit[], Error> {
  const asked = useDebouncedQuery(query);
  return useQuery(() => {
    const root = kiln();
    const text = asked();
    return {
      queryKey: keys.searchSemantic(root ?? '', text),
      queryFn: () => semanticSearch(root ?? '', text, SEMANTIC_LIMIT),
      enabled: root !== null && text.length > 0,
      staleTime: SEARCH_STALE_MS,
    };
  }, getQueryClient);
}

/** Literal content matches under one absolute root, optionally by name. */
export function useGrepSearch(
  root: Accessor<string | null>,
  query: Accessor<string>,
  options?: { glob?: string },
): UseQueryResult<GrepResponse, Error> {
  const asked = useDebouncedQuery(query);
  return useQuery(() => {
    const base = root();
    const text = asked();
    return {
      ...grepOptions(base ?? '', text, options?.glob),
      enabled: base !== null && text.length > 0,
    };
  }, getQueryClient);
}

/**
 * Sessions matching one query, optionally scoped to a kiln.
 *
 * The kiln here is a registry NAME, not a directory: the daemon refuses a set
 * that names kilns and resolves none of them, so a path is a 422. It is in the
 * key for the same reason it is in the question.
 */
export function useSearchSessions(
  query: Accessor<string>,
  kiln: Accessor<string | undefined>,
): UseQueryResult<Session[], Error> {
  const asked = useDebouncedQuery(query);
  return useQuery(() => {
    const scope = kiln();
    const text = asked();
    return {
      queryKey: keys.searchSessions(text, scope),
      queryFn: () => searchSessions(text, scope, SESSION_LIMIT),
      enabled: text.length > 0,
      staleTime: SEARCH_STALE_MS,
    };
  }, getQueryClient);
}
