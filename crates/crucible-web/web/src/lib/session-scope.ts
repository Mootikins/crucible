import type { Session } from '@/lib/types';

/**
 * The one kiln to show when a session must be labelled by a single one.
 *
 * A session's `kilns` is flat — no member is privileged for *scope*. But a
 * tree row has room for one name, so it borrows the first attached kiln, the
 * same choice the daemon's `Session::default_kiln` makes.
 *
 * `null` for a kiln-less session, which is a legitimate shape (a tools-only
 * agent), not a degenerate one. Callers must keep it `null` rather than
 * coercing to `''`: an empty path reaches `kilnLabel()` as the home data dir
 * and renders "Home kiln", inventing an attachment the session does not have.
 * `?.` because a payload that predates the field must read as no kilns, not
 * throw.
 */
export function sessionDefaultKiln(session: Pick<Session, 'kilns'>): string | null {
  return session.kilns?.[0] ?? null;
}

/**
 * Whether the session has a workspace distinct from its knowledge scope.
 *
 * The daemon writes `workspace = kilns[0]` at creation as its "no workspace"
 * sentinel (`Session::new`), so equality here means "no project", not "the
 * project happens to be the kiln". Spelled once so the sentinel rule lives in
 * one place on this side of the wire too.
 *
 * A kiln-less session has no `kilns[0]` for the sentinel to be, and
 * `Session::new` falls back to `PathBuf::default()` — the empty string. The
 * `!!session.workspace` guard is what keeps that from reading as a project
 * directory; such a session still has a real workspace whenever one was set.
 */
export function sessionHasWorkspace(session: Pick<Session, 'kilns' | 'workspace'>): boolean {
  return !!session.workspace && session.workspace !== sessionDefaultKiln(session);
}
