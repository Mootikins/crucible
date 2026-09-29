/** The sessions rail on the mock store. A port reads `useSessionSafe()` and `sessionStatus` instead. */
import { createMemo, type Component } from 'solid-js';
import { openSession } from '../actions';
import { SessionList } from '../components/sessions/SessionList';
import type { SessionGroupView, SessionRowView } from '../components/sessions/types';
import { pendingHunks, state } from '../state';

/** A row view that reads the store on each access, so a row updates in place. */
const rowView = (sid: string): SessionRowView => ({
  id: sid,
  get title() {
    return state.sessions[sid]!.title;
  },
  get status() {
    return state.sessions[sid]!.status;
  },
  get color() {
    return state.sessions[sid]!.color;
  },
  get time() {
    return state.sessions[sid]!.time;
  },
  get pending() {
    return pendingHunks(sid).length;
  },
});

export const SessionsPanelContainer: Component = () => {
  const groups = createMemo(() => {
    const out: Record<string, string[]> = {};
    for (const [sid, s] of Object.entries(state.sessions)) (out[s.group] ??= []).push(sid);
    return Object.entries(out).map(([label, sids]): SessionGroupView => ({ label, sessions: sids.map(rowView) }));
  });
  return <SessionList groups={groups()} activeId={state.active} onOpen={openSession} />;
};
