import type { KilnListEntry } from '@/lib/types';

/**
 * Name ↔ path lookups against the kiln registry the daemon publishes.
 *
 * A kiln crosses the wire as a **name** now — `Session.kilns`, `session.create`,
 * `session.connect_kiln`, the session-search filter. But the browser still has
 * genuinely path-shaped work to do: grep a directory, resolve a wikilink inside
 * a corpus, list a kiln's notes. `GET /api/kilns` is the one endpoint whose job
 * is to say where a kiln lives (`{ name, path }`, the documented exception to
 * "no paths in the API"), so it is the one place the two spellings are joined.
 *
 * The rule every caller here depends on: **a name the registry does not answer
 * for resolves to `null`, and `null` is not a root.** Coercing it to `''`
 * hands `grepSearch` and `listNotes` the empty path, which is the daemon data
 * dir to every path helper — an unresolvable kiln would then search the user's
 * whole `~/.crucible`. Callers must gate on `null`, not fall back.
 */

/**
 * The directory a kiln name points at, or `null` when the registry has no entry
 * under that name.
 *
 * `null` on every uncertain input — no name, no registry yet (the list is still
 * loading), a name that is not in the list. The caller has to decide what "no
 * directory" means for it; every one of them decides "search nothing", never
 * "search everything".
 */
export function kilnPathForName(
  name: string | null | undefined,
  kilns: readonly KilnListEntry[],
): string | null {
  if (!name) return null;
  return kilns.find((k) => k.name === name)?.path ?? null;
}

/**
 * The name a directory is registered under, or `null` when none is.
 *
 * The reverse direction, for the surfaces that still *start* from a path: a
 * focused file's owning kiln, the configured `kiln_path` the shell boots with.
 * An unregistered directory has no name, and inventing one — the basename, say
 * — would produce a name the daemon refuses, which is worse than no name at
 * all because it looks like it worked.
 */
export function kilnNameForPath(
  path: string | null | undefined,
  kilns: readonly KilnListEntry[],
): string | null {
  if (!path) return null;
  const trimmed = path.replace(/\/+$/, '');
  return kilns.find((k) => k.path.replace(/\/+$/, '') === trimmed)?.name ?? null;
}

/**
 * The rows a picker may offer: the ones the daemon says it can resolve.
 *
 * The client half of a rule the daemon states on its own side — `kiln.list`
 * never publishes a name `session.connect_kiln` refuses. A row it cannot name
 * carries `registered: false`, and offering it posts a name the daemon answers
 * 422 to, which the user reads as a broken picker rather than as a directory
 * that needs registering.
 *
 * Such a row is LEFT OUT rather than shown disabled, because it has nothing to
 * show: its `name` is empty by construction, so a disabled row would render as
 * a blank line with a hint. The directory is still visible where directories
 * belong — the file tree — and `cru kiln register` is how it gains a name.
 *
 * Two independent reasons a row is unusable, and both are checked here so no
 * caller has to remember either: the daemon said it is not registered, or
 * there is no name to send.
 */
export function attachableKilns(
  kilns: readonly KilnListEntry[],
): (KilnListEntry & { name: string })[] {
  return kilns.filter(
    (k): k is KilnListEntry & { name: string } => k.registered !== false && !!k.name?.trim(),
  );
}
