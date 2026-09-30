/**
 * The view model of the sessions list.
 *
 * The real app lists sessions through `useSessionSafe()` and `useSessions`
 * (`lib/query/sessions.ts`), and derives three states in
 * `lib/session-status.ts`: `waiting` (here `need`), `working` (`run`) and
 * `idle`. The mockup adds `owe`: an idle session whose edits wait for review
 * (`reviewStore.unreviewedCount` > 0).
 */
export type SessionMarkStatus = 'need' | 'run' | 'owe' | 'idle';

export interface SessionRowView {
  id: string;
  title: string;
  status: SessionMarkStatus;
  /** The identity colour. The real `Session` has no colour field. */
  color: string;
  /** When the session last moved; an idle row shows it. */
  time: string;
  /** The hunks that wait for review. */
  pending: number;
}

export interface SessionGroupView {
  /** The project name, or "Chats" for the sessions that have no project. */
  label: string;
  /** Does the group show its sessions? A fold per project, as `SessionTree` keeps one per group key. */
  open: boolean;
  sessions: SessionRowView[];
}
