/**
 * The proposal surface, over the axum bridge.
 *
 * A proposal is a set of note writes that waits for the user. The daemon owns
 * every decision. These calls only name the proposal and, for a decision on
 * some of its files, those files, each with its root.
 */
import { rpc } from './api-client';
import type { components } from './api-schema';

type Schemas = components['schemas'];

export type ProposalFile = Schemas['ProposalFile'];
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

/**
 * The writes that a person reviews. A move is two writes, the deletion of the
 * old path and the new file that names it in `moved_from`; it counts once,
 * as the new file.
 */
export function reviewedWrites(proposal: Proposal): ProposedWrite[] {
  const moved = (write: ProposedWrite) =>
    write.remove === true &&
    proposal.writes.some((w) => w.root === write.root && w.moved_from === write.path);
  return proposal.writes.filter((write) => !moved(write));
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
  return rpc('proposal.list', {});
}

/** One proposal, in any state. */
export async function getProposal(id: string): Promise<Proposal> {
  return rpc('proposal.get', { id });
}

/**
 * Write the files of a proposal. With `files`, only those files: the daemon
 * moves them into a new proposal, and the reply is that proposal.
 */
export async function acceptProposal(id: string, files: ProposalFile[] = []): Promise<Proposal> {
  return rpc('proposal.accept', { id, files });
}

/**
 * Reject the files of a proposal. With `files`, only those files: the daemon
 * moves them into a new proposal, and the reply is that proposal.
 */
export async function rejectProposal(
  id: string,
  options: { files?: ProposalFile[]; reason?: string } = {},
): Promise<Proposal> {
  const { files = [], reason } = options;
  return rpc('proposal.reject', { id, files, reason });
}

/**
 * Give the settled text of one conflicted file. The daemon writes the whole
 * proposal when no other file conflicts.
 */
export async function resolveProposal(id: string, path: string, text: string, root?: string): Promise<Proposal> {
  return rpc('proposal.resolve', { id, path, text, root });
}

/**
 * Take the proposal out of the Inbox with no decision. The daemon keeps its
 * file, so the history stays.
 */
export async function dismissProposal(id: string): Promise<Proposal> {
  return rpc('proposal.dismiss', { id });
}
