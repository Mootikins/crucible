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
