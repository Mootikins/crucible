import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { SkillDetail } from '@/lib/api';
import type { SkillSummary } from '@/lib/types';
import { useSkillDetail, useSkillList, useSkillSearch } from '../skills';

const ALPHA: SkillSummary = {
  name: 'alpha',
  scope: 'user',
  description: 'first',
  shadowed_count: 0,
};
const BETA: SkillSummary = { name: 'beta', scope: 'kiln', description: 'second', shadowed_count: 0 };

const ALPHA_DETAIL: SkillDetail = {
  name: 'alpha',
  scope: 'user',
  description: 'first',
  source_path: '/tmp/alpha.md',
  agent: null,
  license: null,
  body: '# Alpha',
};

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useSkillList', () => {
  it('asks for one kiln and holds the answer under that kiln', async () => {
    env = createTestQueryEnv({ 'GET /api/skills': () => ({ skills: [ALPHA] }) });

    const both = inRoot(() => ({
      first: useSkillList(() => '/kilns/main'),
      second: useSkillList(() => '/kilns/main'),
    }));

    await vi.waitFor(() => expect(both.first.data).toEqual([ALPHA]));
    expect(both.second.data).toEqual([ALPHA]);
    expect(env.fetch.calls('GET /api/skills')).toBe(1);
  });

  // The key names the kiln, so two panels on two kilns cannot read each other's
  // skills — which is what one shared list keyed on nothing would have done.
  it('holds a separate list per kiln', async () => {
    env = createTestQueryEnv({
      'GET /api/skills': (request) => ({
        skills: new URL(request.url).searchParams.get('kiln') === '/kilns/main' ? [ALPHA] : [BETA],
      }),
    });

    const [kiln, setKiln] = createSignal('/kilns/main');
    // A second panel stays on the first kiln throughout. It is what keeps that
    // entry in the cache, and it is also the case this key exists for: two
    // panels, two kilns, neither drawing the other's skills.
    const both = inRoot(() => ({
      moving: useSkillList(kiln),
      pinned: useSkillList(() => '/kilns/main'),
    }));

    await vi.waitFor(() => expect(both.moving.data).toEqual([ALPHA]));
    setKiln('/kilns/other');
    await vi.waitFor(() => expect(both.moving.data).toEqual([BETA]));
    expect(both.pinned.data).toEqual([ALPHA]);
    // Back to the first kiln reads the entry the other panel holds.
    setKiln('/kilns/main');
    await vi.waitFor(() => expect(both.moving.data).toEqual([ALPHA]));
    expect(env.fetch.calls('GET /api/skills')).toBe(2);
  });

  // The panel mounts before it knows the kiln: the session's name has to be
  // resolved to a directory first. Asking with no kiln would ask the daemon to
  // list the skills of nowhere.
  it('asks nothing until a kiln is known', async () => {
    env = createTestQueryEnv({ 'GET /api/skills': () => ({ skills: [ALPHA] }) });

    const [kiln, setKiln] = createSignal<string | null>(null);
    const query = inRoot(() => useSkillList(kiln));

    expect(env.fetch.calls('GET /api/skills')).toBe(0);
    setKiln('/kilns/main');
    await vi.waitFor(() => expect(query.data).toEqual([ALPHA]));
  });

  it('surfaces a refusal as an error rather than as an empty list', async () => {
    env = createTestQueryEnv({
      'GET /api/skills': apiError(500, 'the kiln is not readable'),
    });

    const query = inRoot(() => useSkillList(() => '/kilns/main'));

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list skills');
  });
});

describe('useSkillSearch', () => {
  it('asks only once the query says something', async () => {
    env = createTestQueryEnv({ 'GET /api/skills/search': () => ({ skills: [BETA] }) });

    const [text, setText] = createSignal('');
    const query = inRoot(() => useSkillSearch(() => '/kilns/main', text));

    expect(env.fetch.calls('GET /api/skills/search')).toBe(0);
    // Whitespace is not a query: the daemon would match everything.
    setText('   ');
    expect(env.fetch.calls('GET /api/skills/search')).toBe(0);

    setText('be');
    await vi.waitFor(() => expect(query.data).toEqual([BETA]));
  });

  it('sends the query and the kiln the caller named', async () => {
    let asked: URLSearchParams | null = null;
    env = createTestQueryEnv({
      'GET /api/skills/search': (request) => {
        asked = new URL(request.url).searchParams;
        return { skills: [BETA] };
      },
    });

    const query = inRoot(() => useSkillSearch(() => '/kilns/main', () => 'be'));

    await vi.waitFor(() => expect(query.data).toEqual([BETA]));
    expect(asked!.get('q')).toBe('be');
    expect(asked!.get('kiln')).toBe('/kilns/main');
  });
});

describe('useSkillDetail', () => {
  it('asks for the picked skill, and asks again for the next one', async () => {
    env = createTestQueryEnv({
      'GET /api/skills/alpha': () => ALPHA_DETAIL,
      'GET /api/skills/beta': () => ({ ...ALPHA_DETAIL, name: 'beta', body: '# Beta' }),
    });

    const [name, setName] = createSignal<string | null>(null);
    // The second reader holds `alpha` open, so the entry survives the trip to
    // `beta` and back — a drawer re-opened on a skill just read draws it from
    // the cache rather than reading the file again.
    const both = inRoot(() => ({
      drawer: useSkillDetail(name, () => '/kilns/main'),
      pinned: useSkillDetail(() => 'alpha', () => '/kilns/main'),
    }));

    // Nothing is picked in the drawer, and the pinned reader asks once.
    await vi.waitFor(() => expect(both.pinned.data?.body).toBe('# Alpha'));

    setName('alpha');
    await vi.waitFor(() => expect(both.drawer.data?.body).toBe('# Alpha'));

    setName('beta');
    await vi.waitFor(() => expect(both.drawer.data?.body).toBe('# Beta'));

    setName('alpha');
    await vi.waitFor(() => expect(both.drawer.data?.body).toBe('# Alpha'));
    expect(env.fetch.calls('GET /api/skills/alpha')).toBe(1);
  });
});
