/** The inbox popover on the mock store. Each answer closes the popover. */
import { mapArray, type Component } from 'solid-js';
import { openChanges } from '../actions';
import { basename } from '../components/path';
import { InboxList, type InboxPermissionView, type InboxReviewView } from '../components/rail/InboxList';
import { answerPermission, pendingHunks, setState, state } from '../state';

/** The sessions whose edits wait for review, and which ask for nothing else. */
export const reviewWaiting = () => Object.keys(state.sessions).filter((sid) => !state.perms[sid] && pendingHunks(sid).length);

/** The count that the inbox button shows. */
export const waitingCount = () => Object.keys(state.perms).length + reviewWaiting().length;

// The views read the store in getters, so a row updates in place. A spread
// would copy the values once, so each view spells out its getters.
const permissionView = (sid: string): InboxPermissionView => ({
  id: sid,
  get title() {
    return state.sessions[sid]!.title;
  },
  get color() {
    return state.sessions[sid]!.color;
  },
  get file() {
    return basename(state.perms[sid]!.path);
  },
});

const reviewView = (sid: string): InboxReviewView => ({
  id: sid,
  get title() {
    return state.sessions[sid]!.title;
  },
  get color() {
    return state.sessions[sid]!.color;
  },
  get pending() {
    return pendingHunks(sid).length;
  },
});

export const InboxContainer: Component<{ onDone: () => void }> = (props) => {
  const permissions = mapArray(() => Object.keys(state.perms), permissionView);
  const reviews = mapArray(reviewWaiting, reviewView);
  const done = (act: () => void) => {
    act();
    props.onDone();
  };
  return (
    <InboxList
      permissions={permissions()}
      reviews={reviews()}
      onAllow={(sid) => done(() => answerPermission(sid, 'once'))}
      onDeny={(sid) => done(() => answerPermission(sid, 'deny'))}
      onOpen={(sid) => done(() => setState('active', sid))}
      onReview={(sid) => done(() => openChanges(sid))}
    />
  );
};
