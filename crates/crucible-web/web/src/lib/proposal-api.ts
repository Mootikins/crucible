/**
 * The proposal surface, over the axum bridge.
 *
 * A proposal is a set of note writes that waits for the user. The daemon owns
 * every decision. These calls only name the proposal and, for a decision on
 * some of its files, the paths.
 */
import { client, decode } from './api-client';
import type { components } from './api-schema';

type Schemas = components['schemas'];

export type Proposal = Schemas['Proposal'];
export type ProposalState = Schemas['ProposalState'];
export type FileConflict = Schemas['FileConflict'];
export type ProposedWrite = Schemas['ProposedWrite'];

/**
 * Whether the user can still accept the proposal: it is not decided, and no
 * newer proposal replaces it. This mirrors `ProposalState::is_pending`.
 */
export function isPending(state: ProposalState): boolean {
  switch (state.kind) {
    case 'open':
    case 'stale':
    case 'conflicted':
      return true;
    case 'superseded':
    case 'accepted':
    case 'rejected':
    case 'dismissed':
      return false;
    default:
      return unknownState(state);
  }
}

/** A short sentence for the state, for the header of the diff pane. */
export function stateLabel(state: ProposalState): string {
  switch (state.kind) {
    case 'open':
      return 'Open';
    case 'stale':
      return 'Stale: the note changed on disk. Accept merges the change.';
    case 'conflicted':
      return 'Conflicted: settle each region, then accept the resolution.';
    case 'accepted':
      return 'Accepted';
    case 'rejected':
      return state.reason ? `Rejected: ${state.reason}` : 'Rejected';
    case 'superseded':
      return 'Superseded by a newer proposal';
    case 'dismissed':
      return 'Dismissed';
    default:
      return unknownState(state);
  }
}

/** Fails the compile when a new state has no branch here. */
function unknownState(state: never): never {
  throw new Error(`Unknown proposal state: ${JSON.stringify(state)}`);
}

/**
 * The name of the writer, for a row or a bar. A plugin pass shows its plugin.
 * A session shows the end of its id.
 */
export function authorLabel(proposal: Proposal): string {
  const author = proposal.author;
  switch (author.kind) {
    case 'plugin':
      return author.name;
    case 'session':
      return `session ${author.id.slice(-8)}`;
  }
}

/** The absolute path of one write: its kiln root and its relative path. */
export function writePath(write: ProposedWrite): string {
  return `${write.root.replace(/\/+$/, '')}/${write.path.replace(/^\/+/, '')}`;
}

/**
 * The proposals in the Inbox, oldest first: open, stale, conflicted and
 * superseded. Accept, reject and dismiss take a proposal out.
 */
export async function listProposals(): Promise<Proposal[]> {
  return decode(await client.GET('/api/proposals', {}), 'Failed to load the proposals');
}

/** One proposal, in any state. */
export async function getProposal(id: string): Promise<Proposal> {
  return decode(
    await client.GET('/api/proposals/{id}', { params: { path: { id } } }),
    'Failed to load the proposal',
  );
}

/**
 * Write the files of a proposal. With `paths`, only those files: the daemon
 * moves them into a new proposal, and the reply is that proposal.
 */
export async function acceptProposal(id: string, paths: string[] = []): Promise<Proposal> {
  return decode(
    await client.POST('/api/proposals/{id}/accept', {
      params: { path: { id } },
      body: paths.length > 0 ? { paths } : {},
    }),
    'Failed to accept the proposal',
  );
}

/**
 * Reject the files of a proposal. With `paths`, only those files: the daemon
 * moves them into a new proposal, and the reply is that proposal.
 */
export async function rejectProposal(
  id: string,
  options: { paths?: string[]; reason?: string } = {},
): Promise<Proposal> {
  const { paths = [], reason } = options;
  return decode(
    await client.POST('/api/proposals/{id}/reject', {
      params: { path: { id } },
      body: { ...(paths.length > 0 ? { paths } : {}), ...(reason ? { reason } : {}) },
    }),
    'Failed to reject the proposal',
  );
}

/**
 * Give the settled text of one conflicted file. The daemon writes the whole
 * proposal when no other file conflicts.
 */
export async function resolveProposal(id: string, path: string, text: string): Promise<Proposal> {
  return decode(
    await client.POST('/api/proposals/{id}/resolve', {
      params: { path: { id } },
      body: { path, text },
    }),
    'Failed to accept the resolution',
  );
}

/**
 * Take the proposal out of the Inbox with no decision. The daemon keeps its
 * file, so the history stays.
 */
export async function dismissProposal(id: string): Promise<Proposal> {
  return decode(
    await client.POST('/api/proposals/{id}/dismiss', { params: { path: { id } } }),
    'Failed to dismiss the proposal',
  );
}
