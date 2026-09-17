import { useMutation, type UseMutationResult } from '@tanstack/solid-query';
import { login } from '@/lib/api';
import { getQueryClient } from './client';

/**
 * Sign-in, as the prompt and the settings page call it.
 *
 * Deliberately NOT a cache entry, and deliberately no invalidation: the
 * exchange mints an HttpOnly session cookie and the page reloads, so there is
 * no key to hold and nothing stale to drop. The hook exists for the import
 * rule — a component reads the query layer, never the api module — and
 * nothing else: it owns no state a caller does not already hold.
 *
 * Answers whether the daemon took the key; a caller that gets `true` should
 * reload so every context refetches with credentials.
 */
export function useLogin(): UseMutationResult<boolean, Error, string, unknown> {
  return useMutation(
    () => ({ mutationFn: (key: string) => login(key) }),
    () => getQueryClient(),
  );
}
