import type { Accessor } from 'solid-js';
import { useMutation, useQuery, type UseMutationResult, type UseQueryResult } from '@tanstack/solid-query';
import { diffsetKey } from '@/lib/diffset';
import {
  acceptProposal,
  getProposal,
  rejectProposal,
  resolveProposal,
  type Proposal,
} from '@/lib/proposal-api';
import { getQueryClient } from './client';
import { keys } from './keys';

/** One proposal, with its state. */
export function useProposal(id: Accessor<string>): UseQueryResult<Proposal, Error> {
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
  | { kind: 'accept'; paths?: string[] }
  | { kind: 'reject'; paths?: string[] }
  | { kind: 'resolve'; path: string; text: string };

/**
 * Makes the proposal `id` and its diffset wrong. A decision changes the
 * state, and a decision on some files changes the file list.
 */
export function invalidateProposal(id: string): Promise<void> {
  const client = getQueryClient();
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.proposal(id) }),
    client.invalidateQueries({ queryKey: keys.diffset(diffsetKey({ kind: 'proposal', id })) }),
  ]).then(() => undefined);
}

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
            return acceptProposal(value, decision.paths);
          case 'reject':
            return rejectProposal(value, { paths: decision.paths });
          case 'resolve':
            return resolveProposal(value, decision.path, decision.text);
        }
      },
      onSettled: (reply) => {
        const ids = new Set([id(), ...(reply ? [reply.id] : [])]);
        return Promise.all([...ids].map(invalidateProposal));
      },
    }),
    getQueryClient,
  );
}
