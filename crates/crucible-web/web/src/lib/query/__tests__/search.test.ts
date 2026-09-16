import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { keys } from '../keys';
import {
  SEARCH_DEBOUNCE_MS,
  useGrepSearch,
  useSearchSessions,
  useSemanticSearch,
} from '../search';

/**
 * The three searches, each held under the question it answers.
 *
 * Two behaviours the panel used to carry by hand are asserted here. It waited
 * out the typing before it asked, and it carried a token so a slow answer for
 * a query the user had moved past did not overwrite a fast answer for the
 * current one. The first is the debounce below; the second is what a KEY
 * gives for free, and the last case is what says so.
 */

const KILN = '/kiln';

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
/** The query text of every search the daemon answered, per route. */
let asked: { route: string; query: string; root: string }[] = [];
/** Answers that wait, by query text, so a slow one can land after a fast one. */
let held = new Map<string, () => void>();

function searchRoutes() {
  asked = [];
  held = new Map();
  const wait = (query: string): Promise<void> =>
    held.has(query)
      ? new Promise<void>((resolve) => held.set(query, resolve))
      : Promise.resolve();
  return {
    'POST /api/search/semantic': async (request: Request) => {
      const body = (await request.json()) as { kiln: string; query: string };
      asked.push({ route: 'semantic', query: body.query, root: body.kiln });
      await wait(body.query);
      return { results: [{ path: `/a/${body.query}.md`, rel_path: 'a.md', document_id: 'd', score: 1 }] };
    },
    'POST /api/search/grep': async (request: Request) => {
      const body = (await request.json()) as { root: string; query: string; glob: string | null };
      asked.push({ route: 'grep', query: body.query, root: body.root });
      await wait(body.query);
      return {
        hits: [
          { path: `/a/${body.query}`, rel_path: 'a.md', line: 1, text: body.query, match_start: 0, match_end: 1 },
        ],
        truncated: false,
      };
    },
    'GET /api/sessions/search': (request: Request) => {
      const params = new URL(request.url).searchParams;
      asked.push({ route: 'sessions', query: params.get('q') ?? '', root: params.get('kiln') ?? '' });
      return [];
    },
  };
}

const countOf = (route: string) => asked.filter((seen) => seen.route === route).length;

/**
 * Keeps an entry the last reader left, the way the app does.
 *
 * The test client drops one the moment nobody observes it, so a case about
 * coming BACK to a query has to hold them the app's way or it asserts the
 * test harness instead of the panel.
 */
function holdUnobservedEntries(): void {
  env.client.setDefaultOptions({
    queries: { gcTime: 60_000, retry: false, refetchOnWindowFocus: false },
  });
}

function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

beforeEach(() => {
  env = createTestQueryEnv(searchRoutes());
});

afterEach(() => {
  vi.useRealTimers();
  dispose?.();
  dispose = null;
  env?.restore();
});

describe('the debounce', () => {
  it('asks once for a word typed one letter at a time', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const [text, setText] = createSignal('');
    const query = inRoot(() => useSemanticSearch(() => KILN, text));

    for (const partial of ['r', 'ru', 'rus', 'rust']) {
      setText(partial);
      await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS / 4);
    }
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS);

    await waitFor(() => expect(query.data).toBeDefined());
    expect(asked.map((seen) => seen.query)).toEqual(['rust']);
  });

  /**
   * Clearing the box is not a search.
   *
   * Making the user watch the old hits for a fifth of a second after the box
   * is empty says the panel did not notice they cleared it.
   */
  it('drops the query the moment the box is emptied', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const [text, setText] = createSignal('rust');
    const query = inRoot(() => useSemanticSearch(() => KILN, text));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);
    await waitFor(() => expect(query.data).toBeDefined());

    setText('');

    expect(query.data).toBeUndefined();
    expect(countOf('semantic')).toBe(1);
  });

  it('asks for nothing at all while the box is empty', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const query = inRoot(() => useSemanticSearch(() => KILN, () => ''));

    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS * 3);

    expect(query.data).toBeUndefined();
    expect(asked).toEqual([]);
  });
});

describe('the key', () => {
  it('answers a query asked again without asking the daemon', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    holdUnobservedEntries();
    const [text, setText] = createSignal('rust');
    const query = inRoot(() => useSemanticSearch(() => KILN, text));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);
    await waitFor(() => expect(query.data).toBeDefined());

    // One character more, then back: the panel used to re-run the whole search
    // for a query it had just shown.
    setText('rusty');
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);
    await waitFor(() => expect(countOf('semantic')).toBe(2));
    setText('rust');
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);

    await waitFor(() => expect(query.data?.[0].path).toBe('/a/rust.md'));
    expect(countOf('semantic')).toBe(2);
  });

  /**
   * The stale-request guard, which is what a key IS.
   *
   * The panel carried a `runToken` for this: a slow answer for a query the
   * user has moved past must not overwrite a fast answer for the one they are
   * looking at. An answer belongs to its question now, so the late one lands
   * in an entry nobody is reading.
   */
  it('does not let a slow answer overwrite the query that came after it', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    held.set('slow', () => {});
    const [text, setText] = createSignal('slow');
    const query = inRoot(() => useGrepSearch(() => KILN, text));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);
    await waitFor(() => expect(countOf('grep')).toBe(1));

    setText('fast');
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);
    await waitFor(() => expect(query.data?.hits[0].text).toBe('fast'));

    // The first answer lands now, long after the user moved on.
    held.get('slow')!();
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(query.data?.hits[0].text).toBe('fast');
  });

  it('holds a grep of one root apart from the same words in another', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const both = inRoot(() => ({
      notes: useGrepSearch(() => KILN, () => 'rust', { glob: '*.md' }),
      files: useGrepSearch(() => '/proj', () => 'rust'),
    }));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);

    await waitFor(() => expect(both.notes.data).toBeDefined());
    await waitFor(() => expect(both.files.data).toBeDefined());
    expect(asked.filter((seen) => seen.route === 'grep').map((seen) => seen.root)).toEqual([
      KILN,
      '/proj',
    ]);
    expect(env.client.getQueryData(keys.searchGrep(KILN, 'rust', '*.md'))).toEqual(
      both.notes.data,
    );
  });

  it('holds the same words under two globs apart', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const both = inRoot(() => ({
      notes: useGrepSearch(() => KILN, () => 'rust', { glob: '*.md' }),
      everything: useGrepSearch(() => KILN, () => 'rust'),
    }));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);

    await waitFor(() => expect(both.notes.data).toBeDefined());
    await waitFor(() => expect(both.everything.data).toBeDefined());
    expect(countOf('grep')).toBe(2);
  });
});

describe('useSearchSessions', () => {
  it('scopes by the kiln NAME and holds each scope apart', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const both = inRoot(() => ({
      everywhere: useSearchSessions(() => 'rust', () => undefined),
      scoped: useSearchSessions(() => 'rust', () => 'helios'),
    }));
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS + 10);

    await waitFor(() => expect(both.everywhere.data).toBeDefined());
    await waitFor(() => expect(both.scoped.data).toBeDefined());
    expect(asked.filter((seen) => seen.route === 'sessions').map((seen) => seen.root)).toEqual([
      '',
      'helios',
    ]);
  });
});
