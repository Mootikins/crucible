import type { Proposal, ProposalState } from '@/lib/proposal-api';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';

/**
 * One proposal as the daemon sends it: a plugin pass that changes one note
 * under `/kiln`. A case names only the fields that it asserts on.
 */
export function proposalFixture(
  id: string,
  state: ProposalState = { kind: 'open' },
  over: Partial<Proposal> = {},
): Proposal {
  return {
    id,
    author: { kind: 'plugin', name: 'consolidation' },
    title: `Proposal ${id}`,
    created_at: '2026-09-21T10:00:00Z',
    state,
    writes: [
      {
        root: '/kiln',
        path: 'notes/a.md',
        base: { kind: 'hash', hash: 'h0' },
        new_text: 'a\n',
      },
    ],
    ...over,
  } as Proposal;
}

/**
 * The routes that the Inbox list and the proposal diffsets read. Each diffset
 * lists the writes of its proposal, with `counts` as the added and the
 * removed lines of each file.
 */
export function proposalRoutes(
  proposals: Proposal[],
  counts: { added: number; removed: number } = { added: 3, removed: 1 },
): Record<string, MockFetchAnswer> {
  return {
    'GET /api/proposals': () => proposals,
    'GET /api/diff': (request: Request) => {
      const id = new URL(request.url).searchParams.get('proposal') ?? '';
      const proposal = proposals.find((p) => p.id === id);
      return {
        id: `proposal-${id}`,
        source: { kind: 'proposal', id },
        files: (proposal?.writes ?? []).map((write) => ({
          root: write.root,
          path: write.path,
          status: { kind: 'modified' },
          binary: false,
          too_large: false,
          ...counts,
        })),
        unreadable_roots: [],
      };
    },
  };
}
