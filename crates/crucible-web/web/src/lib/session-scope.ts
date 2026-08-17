import type { Session } from '@/lib/types';

/**
 * The one kiln NAME to show when a session must be labelled by a single one.
 *
 * A session's `kilns` is flat — no member is privileged for *scope*. But a
 * tree row has room for one name, so it borrows the first attached kiln, the
 * same choice the daemon's `Session::default_kiln` makes.
 *
 * A registry name, not a directory. Callers that need the directory (grep,
 * wikilink resolution, the status bar's kiln path) must resolve it through
 * `kilnPathForName()`; passing this straight to a path helper searches a
 * relative directory named after the kiln, which is nowhere.
 *
 * `null` for a kiln-less session, which is a legitimate shape (a tools-only
 * agent), not a degenerate one. Callers must keep it `null` rather than
 * coercing to `''`, which every path helper reads as the daemon data dir —
 * inventing an attachment the session does not have, and a far wider one than
 * any kiln. `?.` because a payload that predates the field must read as no
 * kilns, not throw.
 */
export function sessionDefaultKiln(session: Pick<Session, 'kilns'>): string | null {
  return session.kilns?.[0] ?? null;
}

/**
 * The directory the session acts in, or `null` when it has none.
 *
 * The daemon answers `workspace: null` outright — it used to write
 * `workspace = kilns[0]` as a "no workspace" sentinel, which meant this side
 * kept an INDEPENDENT copy of that rule, in another language, with nothing to
 * fail if the two drifted. Reading the daemon's own answer is the whole point
 * of the change: a workspace that happens to equal the kiln is now a
 * workspace the user chose, and renders as one.
 *
 * The empty string and an absent field collapse to `null` too. Neither names
 * a directory, and both reach `pathBasename()` and the session-tree grouping
 * key as if they did — a payload written before the field became nullable
 * carries `''`.
 */
export function sessionWorkspace(session: Pick<Session, 'workspace'>): string | null {
  return session.workspace || null;
}
