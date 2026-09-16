import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  executePluginOption,
  getPluginCommands,
  getPluginOption,
  getPluginOptions,
  getPluginPublications,
  getPlugins,
  installPlugin,
  reloadPlugin,
  removePlugin,
  runPluginCommand,
  setPluginOption,
  type InstallPluginParams,
  type InstallPluginResult,
  type PluginCommand,
  type PluginInfo,
  type PluginOptions,
  type PluginPublications,
  type PluginReloadResult,
  type RemovePluginResult,
} from '@/lib/api';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * Everything the daemon knows about plugins, held once.
 *
 * Three panels used to hold three copies of it. `PluginPanel` kept a list and
 * an options tree, the settings section kept a SECOND list, and the settings
 * modal kept a SECOND options tree. Nothing connected them, so an install from
 * settings left the panel's roster stale, and a reload in the panel left the
 * modal describing a version of the tree that no longer existed. The comment at
 * `PluginPanel.tsx:164` named that hazard and could not fix it from inside one
 * component.
 *
 * One key per question fixes it: every reader mounts the same entry, and every
 * write invalidates the entries it made wrong. A plugin installed anywhere now
 * appears everywhere, with its settings pane, without a reload of the page.
 */

/**
 * The three keys a plugin's arrival, departure or reload makes wrong.
 *
 * All three, for all three writes. An install adds a roster row AND the
 * settings tree that row's pane draws AND the commands the plugin declares; a
 * removal takes the same three away; a reload replaces the tree (its accessors
 * close over the previous load's state) and can change either count. Naming
 * fewer keys is how the old code left a pane describing a plugin that was gone.
 *
 * The promise is RETURNED rather than dropped, so a mutation settles only once
 * the new answers have landed. A caller that re-reads the roster the instant
 * its install resolves would otherwise read the roster as it stood before.
 */
function invalidateRoster(client: QueryClient): Promise<void> {
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.pluginList() }),
    client.invalidateQueries({ queryKey: keys.pluginOptions() }),
    client.invalidateQueries({ queryKey: keys.pluginCommands() }),
  ]).then(() => undefined);
}

/**
 * Every option value of one plugin.
 *
 * `keys.pluginOption(plugin, [])` is the prefix of every path under that
 * plugin, and the cache matches keys by prefix, so this reaches the row that
 * changed and its siblings together. Siblings are the point: an option's
 * `values` or `disabled` field is a FUNCTION the daemon runs, and it may read
 * the option just written, so a write can change a row it did not name.
 */
function invalidateOptions(client: QueryClient, plugin: string): Promise<void> {
  return client.invalidateQueries({ queryKey: keys.pluginOption(plugin, []) });
}

/** The discovered plugins, with their state and their capability counts. */
export function usePluginList(): UseQueryResult<PluginInfo[], Error> {
  return useQuery(
    () => ({ queryKey: keys.pluginList(), queryFn: getPlugins }),
    getQueryClient,
  );
}

/**
 * The settings tree every plugin declared, rendered for this frontend.
 *
 * One fetch for the panel, the settings modal and the phone's settings list
 * together. Describing a tree runs its function-valued fields — `oci` shells
 * out to find its installed runtimes — so this is a real cost, and paying it
 * three times was the reason the modal only asked once it opened.
 *
 * `enabled` is for a reader that is mounted before it is on screen. The
 * settings dialog is one: it sits in the shell from boot and draws nothing
 * until `open`, so asking on mount would run every plugin's describe at
 * start-up. It keeps the same key, so the first reader that IS on screen
 * fetches and every other one reads that answer.
 */
export function usePluginOptions(
  enabled?: Accessor<boolean>,
): UseQueryResult<PluginOptions, Error> {
  return useQuery(
    () => ({
      queryKey: keys.pluginOptions(),
      queryFn: getPluginOptions,
      enabled: enabled?.() ?? true,
    }),
    getQueryClient,
  );
}

/** Every command loaded plugins declared. */
export function usePluginCommands(): UseQueryResult<PluginCommand[], Error> {
  return useQuery(
    () => ({ queryKey: keys.pluginCommands(), queryFn: getPluginCommands }),
    getQueryClient,
  );
}

/**
 * What plugins published, for one key, as one plugin.
 *
 * Both arguments narrow: the key narrows daemon-side, and the caller reaches
 * the route's per-plugin comparison. A reader that gives neither asks for
 * everything, which is what the block panel's roster needs and no block does.
 *
 * The key names the plugin AND the key, because the `publication_changed` route
 * invalidates that pair. A document with four blocks of one plugin holds four
 * entries, and an event about one of them throws away one of them.
 */
export function usePluginPublications(
  plugin?: string,
  key?: string,
): UseQueryResult<PluginPublications, Error> {
  return useQuery(
    () => ({
      queryKey: keys.pluginPublications(plugin, key),
      queryFn: () => getPluginPublications(key, plugin),
    }),
    getQueryClient,
  );
}

/**
 * One option's current value.
 *
 * The plugin and the path arrive as accessors because a settings row outlives
 * the declaration it draws: the tree reloads into the same `Index` position and
 * the row's path can move with it. Read once at mount, the row would go on
 * asking about the option it drew a minute ago.
 */
export function usePluginOption(
  plugin: Accessor<string>,
  path: Accessor<readonly string[]>,
): UseQueryResult<unknown, Error> {
  return useQuery(
    () => ({
      queryKey: keys.pluginOption(plugin(), path()),
      queryFn: () => getPluginOption(plugin(), [...path()]),
    }),
    getQueryClient,
  );
}

/**
 * Writes one option, painting the new value before the plugin answers.
 *
 * The optimistic write and its undo are the mutation's, not the row's: a
 * setter is free to normalise, to clamp or to refuse, so the pane must end up
 * showing what the plugin STORED rather than what was typed at it. On success
 * every option of the plugin is asked again, which is the reconciliation; on a
 * refusal the snapshot goes back, so a rejected edit does not sit on screen
 * looking accepted.
 */
export function useSetPluginOption(
  plugin: Accessor<string>,
  path: Accessor<readonly string[]>,
): UseMutationResult<void, Error, unknown, { key: readonly unknown[]; previous: unknown }> {
  return useMutation(
    () => ({
      mutationFn: (value: unknown) => setPluginOption(plugin(), [...path()], value),
      onMutate: async (value: unknown) => {
        const client = getQueryClient();
        const key = keys.pluginOption(plugin(), path());
        // A read in flight would land after the optimistic write and paint the
        // old value over it.
        await client.cancelQueries({ queryKey: key });
        const previous = client.getQueryData(key);
        client.setQueryData(key, value);
        return { key, previous };
      },
      onError: (_error, _value, context) => {
        if (context) getQueryClient().setQueryData(context.key, context.previous);
      },
      onSuccess: () => invalidateOptions(getQueryClient(), plugin()),
    }),
    getQueryClient,
  );
}

/**
 * Presses a `type = "execute"` node.
 *
 * Nothing is painted first, because an action declares no value to paint. The
 * plugin's own settings are asked again afterwards: pressing "install the
 * runtime" is exactly the kind of action that changes what the rows beside it
 * report.
 */
export function useExecutePluginOption(
  plugin: Accessor<string>,
  path: Accessor<readonly string[]>,
): UseMutationResult<void, Error, void> {
  return useMutation(
    () => ({
      mutationFn: () => executePluginOption(plugin(), [...path()]),
      onSuccess: () => invalidateOptions(getQueryClient(), plugin()),
    }),
    getQueryClient,
  );
}

/** What one command invocation needs: its name, its arguments, and who asks. */
export interface RunPluginCommand {
  command: string;
  args?: unknown;
  /**
   * The plugin the caller draws for, or absent for the app itself.
   *
   * A block declares itself as its own plugin so the route can refuse a block
   * reaching for someone else's command. It is an assertion, not a proof — see
   * `routes/plugin_caller.rs`.
   */
  caller?: string;
}

/**
 * Invokes a plugin command.
 *
 * Nothing is invalidated here, and nothing can be: the command is the plugin's
 * own verb and only the plugin knows what it touched. A command that changes
 * published data republishes, and the `publication_changed` route writes the
 * cache from the stream.
 */
export function useRunPluginCommand(): UseMutationResult<unknown, Error, RunPluginCommand> {
  return useMutation(
    () => ({
      mutationFn: ({ command, args, caller }: RunPluginCommand) =>
        // Omitted rather than passed as `undefined`, so the app's own caller
        // name comes from the one place that declares it.
        caller === undefined
          ? runPluginCommand(command, args ?? {})
          : runPluginCommand(command, args ?? {}, caller),
    }),
    getQueryClient,
  );
}

/**
 * Installs a plugin from a URL.
 *
 * Slow — a clone over the network, with no timeout beyond `fetch`'s, so every
 * caller shows a spinner. `installed` and `loaded` are separate answers and the
 * caller must read both: a plugin can be recorded in the manifest and still
 * sit broken on the running daemon.
 */
export function useInstallPlugin(): UseMutationResult<
  InstallPluginResult,
  Error,
  InstallPluginParams
> {
  return useMutation(
    () => ({
      mutationFn: (params: InstallPluginParams) => installPlugin(params),
      onSuccess: () => invalidateRoster(getQueryClient()),
    }),
    getQueryClient,
  );
}

/** Reloads one plugin by name. */
export function useReloadPlugin(): UseMutationResult<PluginReloadResult, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (name: string) => reloadPlugin(name),
      onSuccess: () => invalidateRoster(getQueryClient()),
    }),
    getQueryClient,
  );
}

/** What a removal needs: the plugin, and whether its directory goes with it. */
export interface RemovePluginRequest {
  name: string;
  purge: boolean;
}

/** Removes one plugin, and optionally deletes the directory it was cloned to. */
export function useRemovePlugin(): UseMutationResult<
  RemovePluginResult,
  Error,
  RemovePluginRequest
> {
  return useMutation(
    () => ({
      mutationFn: ({ name, purge }: RemovePluginRequest) => removePlugin(name, purge),
      onSuccess: () => invalidateRoster(getQueryClient()),
    }),
    getQueryClient,
  );
}
