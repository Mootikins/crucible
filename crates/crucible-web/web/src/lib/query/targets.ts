import { createMemo, type Accessor } from 'solid-js';
import {
  useMutation,
  useQueries,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  getProviderTargets,
  getTargetProviders,
  listWorkspaceTargets,
  resolveWorkspaceTarget,
} from '@/lib/api';
import type { ProviderTarget, TargetProvider } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The two target axes, as queries.
 *
 * WORKSPACE answers where a session's files live (a worktree, a checkout on
 * another machine); RUNTIME answers where its process runs (a container, an
 * ssh host). Both are contributed by plugins, so every read here costs a
 * publications fetch plus one plugin command per provider.
 *
 * Nine sites paid that bill privately. The composer kept two `swrLocal` lists
 * and re-ran its own fan-out with a hand-written out-of-order guard, the phone
 * sheet ran the same fan-out again, the sessions rail ran one fan-out PER
 * PROJECT on every roster change and on every window focus, and the root
 * dropdown ran another on every popout open. Nothing was shared, so two panels
 * asking the same provider about the same repository asked it twice.
 *
 * Every read below is keyed by what it actually depends on — the axis, the
 * provider and the workspace — so two callers asking the same question join
 * one request, and asking a DIFFERENT question is what makes a new one.
 *
 * Nothing here refetches on window focus. `queryClientOptions` turns that off
 * for every query, deliberately: the daemon pushes a plugin change over SSE,
 * and the rail's focus listener re-ran N plugin commands every time the user
 * came back to the tab.
 */

/** The providers on one axis. */
function providersOptions(axis: TargetProvider['axis']) {
  return {
    queryKey: keys.targetProviders(axis),
    queryFn: () => getTargetProviders(axis),
  };
}

/**
 * One provider's targets for one workspace.
 *
 * Shared by the hook and by the fan-out below, so a single provider read and
 * an axis-wide read land on the same cache entry.
 */
function providerTargetsOptions(provider: TargetProvider, workspace: string | undefined) {
  return {
    queryKey: keys.providerTargets(provider.plugin, provider.axis, workspace),
    queryFn: () => getProviderTargets(provider, workspace),
  };
}

/** Every workspace target on one project root, across providers, flattened. */
function workspaceTargetsOptions(workspace: string | undefined) {
  return {
    queryKey: keys.workspaceTargets(workspace),
    queryFn: () => listWorkspaceTargets(workspace),
    // A root is only a question once there is one. Until then the hook holds
    // no entry rather than fetching every provider's answer for "no project".
    enabled: workspace !== undefined,
  };
}

/** Providers that can resolve targets on the given axis. */
export function useTargetProviders(
  axis: TargetProvider['axis'],
): UseQueryResult<TargetProvider[], Error> {
  return useQuery(() => providersOptions(axis), getQueryClient);
}

/** What one axis offers for one workspace: a target list per provider. */
export interface AxisTargets {
  /** The targets each provider offers, by plugin name. */
  readonly targets: Record<string, ProviderTarget[]>;
  /** True once every provider on the axis has answered — or there are none. */
  readonly ready: boolean;
}

/**
 * Every provider's targets on one axis, for one workspace.
 *
 * The composer and the phone sheet both need the whole axis, and the number of
 * providers is only known once the providers query answers, so this is
 * `useQueries` over that answer rather than a hook per provider. Each provider
 * still gets its own cache entry, keyed by plugin, axis and workspace.
 *
 * The workspace arrives as an accessor because it is the project chip, which
 * the user changes while the component is mounted. Changing it re-keys every
 * query, which is also what replaces the hand-written out-of-order guard the
 * composer used to need: an answer for the project the user left lands in that
 * project's entry, not on screen.
 */
export function useAxisTargets(
  axis: TargetProvider['axis'],
  workspace: Accessor<string | undefined>,
): AxisTargets {
  const providers = useTargetProviders(axis);
  const list = createMemo<TargetProvider[]>(() => providers.data ?? []);
  // No `combine`: this version of `useQueries` holds the combined value in a
  // store it indexes as an array, so a combiner that answers an object breaks
  // it. The results array is combined here instead, which keeps the same one
  // entry per provider.
  const results = useQueries(
    () => ({ queries: list().map((provider) => providerTargetsOptions(provider, workspace())) }),
    getQueryClient,
  );

  const targets = createMemo(() =>
    Object.fromEntries(
      list().flatMap((provider, index) => {
        const data = results[index]?.data;
        return data ? [[provider.plugin, data] as const] : [];
      }),
    ),
  );
  // An axis with no provider is ready with nothing, which is a different
  // statement from "still asking" — the chip renders one and hides for the
  // other.
  const ready = createMemo(
    () => providers.isSuccess && list().every((_, index) => !results[index]?.isPending),
  );

  return {
    get targets() {
      return targets();
    },
    get ready() {
      return ready();
    },
  };
}

/**
 * Every workspace target on one project root, across providers.
 *
 * For the consumers that want the data rather than a menu: the files pane's
 * root picker listing the branches of the active repository.
 */
export function useWorkspaceTargets(
  workspace: Accessor<string | undefined>,
): UseQueryResult<ProviderTarget[], Error> {
  return useQuery(() => workspaceTargetsOptions(workspace()), getQueryClient);
}

/**
 * The same read for several project roots at once, as one map.
 *
 * The sessions rail labels each checkout with its branch, which needs one
 * fan-out per repository root. It used to run them all from an effect that
 * re-ran on every roster change and on every window focus, and it held the
 * answer in a private signal. Each root is now its own cache entry under the
 * key `useWorkspaceTargets` reads, so a rail and a picker looking at the same
 * repository ask once, and a root that did not change is not asked again.
 */
export function useWorkspaceTargetsByRoot(
  roots: Accessor<string[]>,
): Accessor<Map<string, ProviderTarget[]>> {
  const list = createMemo<string[]>(() => roots());
  const results = useQueries(
    () => ({ queries: list().map((root) => workspaceTargetsOptions(root)) }),
    getQueryClient,
  );
  return createMemo(
    () =>
      new Map(
        list().flatMap((root, index) => {
          const data = results[index]?.data;
          return data ? [[root, data] as const] : [];
        }),
      ),
  );
}

/**
 * True for every target entry that answers for one workspace.
 *
 * Two key shapes answer for a workspace and they hold it in different places:
 * `['targets', 'workspace', ws]` and `['targets', 'provider', plugin, axis,
 * ws]`. A prefix match reaches only the first, so the composer's per-provider
 * pickers would keep a list that a new checkout has already made wrong.
 */
function answersForWorkspace(
  queryKey: readonly unknown[],
  workspace: string | undefined,
): boolean {
  if (queryKey[0] !== 'targets') return false;
  if (queryKey[1] === 'workspace') return queryKey[2] === workspace;
  if (queryKey[1] === 'provider') return queryKey[4] === workspace;
  return false;
}

/** What one resolve asks for: the target's spec, and the repo it comes from. */
export interface ResolveTargetRequest {
  spec: string;
  workspace?: string;
}

/**
 * Materialises a checkout for one target spec, and answers with its path.
 *
 * A mutation, not a read: it CREATES the worktree when the provider has none,
 * and a provider that cannot resolve a spec the user explicitly picked has to
 * say so. That is why this one throws where the enumerating reads answer with
 * an empty list.
 *
 * It invalidates every target entry for that workspace, which the plan's
 * signature table leaves blank: the list the caller just read said the target
 * had no checkout, and after this it has one. The files pane reads one key for
 * that list and the two composers read another per provider, so both shapes
 * have to go — see `answersForWorkspace`.
 */
export function useResolveWorkspaceTarget(): UseMutationResult<
  string,
  Error,
  ResolveTargetRequest
> {
  return useMutation(
    () => ({
      mutationFn: ({ spec, workspace }: ResolveTargetRequest) =>
        resolveWorkspaceTarget(spec, workspace),
      onSuccess: (_path, { workspace }) =>
        void getQueryClient().invalidateQueries({
          predicate: (query) => answersForWorkspace(query.queryKey, workspace),
        }),
    }),
    getQueryClient,
  );
}
