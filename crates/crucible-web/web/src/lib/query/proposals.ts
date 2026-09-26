import { onCleanup, type Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { invalidateProposal } from './proposal-cache';
import {
  acceptProposal,
  dismissProposal,
  getProposal,
  listProposals,
  rejectProposal,
  resolveProposal,
  type Proposal,
  type ProposalFile,
} from '@/lib/proposal-api';
import { getQueryClient } from './client';
import { keys } from './keys';
import { systemEvents } from './sse';

/**
 * Holds the system stream open while the caller is on screen.
 *
 * The caller does nothing with the frame. The route of the stream
 * (`routes/system.ts`) invalidates the proposal entries. The root counts its
 * subscribers, and with none it closes the `EventSource`.
 */
function holdSystemStream(): void {
  onCleanup(systemEvents().subscribe(() => {}));
}

/**
 * The proposals in the Inbox, oldest first. The list refetches when the daemon
 * sends `proposal_changed`.
 */
export function useProposals(): UseQueryResult<Proposal[], Error> {
  holdSystemStream();
  return useQuery(
    () => ({
      queryKey: keys.proposals(),
      queryFn: listProposals,
    }),
    getQueryClient,
  );
}

/**
 * One proposal, with its state. The entry refetches when the daemon sends
 * `proposal_changed` for it.
 */
export function useProposal(id: Accessor<string>): UseQueryResult<Proposal, Error> {
  holdSystemStream();
  return useQuery(() => {
    const value = id();
    return {
      queryKey: keys.proposal(value),
      queryFn: () => getProposal(value),
    };
  }, getQueryClient);
}

/** One decision of the diff pane. No paths means every file. */
export type ProposalDecision =
  | { kind: 'accept'; paths?: string[]; files?: ProposalFile[] }
  | { kind: 'reject'; paths?: string[]; files?: ProposalFile[] }
  | { kind: 'resolve'; path: string; root?: string; text: string };

/**
 * Sends one decision on the proposal `id`. The reply is the proposal that
 * holds the decided files: `id` itself, or a new proposal for some files.
 */
export function useProposalDecision(
  id: Accessor<string>,
): UseMutationResult<Proposal, Error, ProposalDecision> {
  return useMutation(
    () => ({
      mutationFn: (decision: ProposalDecision) => {
        const value = id();
        switch (decision.kind) {
          case 'accept':
            return acceptProposal(value, decision.paths, decision.files);
          case 'reject':
            return rejectProposal(value, { paths: decision.paths, files: decision.files });
          case 'resolve':
            return resolveProposal(value, decision.path, decision.text, decision.root);
        }
      },
      onSettled: (reply) => {
        const ids = new Set([id(), ...(reply ? [reply.id] : [])]);
        return Promise.all([...ids].map(value => invalidateProposal(getQueryClient(), value)));
      },
    }),
    getQueryClient,
  );
}

/**
 * Takes a proposal out of the Inbox with no decision. A superseded proposal
 * leaves this way.
 */
export function useDismissProposal(): UseMutationResult<Proposal, Error, string> {
  return useMutation(
    () => ({
      mutationFn: dismissProposal,
      onSettled: (_reply, _error, id) => invalidateProposal(getQueryClient(), id),
    }),
    getQueryClient,
  );
}
