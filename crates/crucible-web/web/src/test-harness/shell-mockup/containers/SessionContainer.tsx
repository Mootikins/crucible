import { proposalPending } from '../review';
/**
 * One session on the mock store, in its own tab. A port reads ChatContext.
 */
import type { Component } from 'solid-js';
import { windowActions, windowStore } from '@/windowing/store';
import { collectLeafGroupIds } from '@/windowing/model/tree';
import { focusComposer, openChanges, withTransition } from '../actions';
import { SessionHeader } from '../components/session/SessionHeader';
import { SessionLayout } from '../components/session/SessionLayout';
import { Transcript } from '../components/session/Transcript';
import { blocks } from '../components/session/blocks';
import { setState, state } from '../state';
import { SessionFooterContainer } from './SessionFooterContainer';
import { mockToolHandlers } from './toolHandlers';
import { noteLinks } from './wikilinks';

export const SessionContainer: Component<{ sid: string }> = (props) => {
  const sid = () => props.sid;
  const s = () => state.sessions[sid()]!;
  const expanded = () => windowStore.expandedEdge === 'right';
  // Only a session in the right rail can cover the centre.
  const inRail = () =>
    collectLeafGroupIds(windowStore.edgePanels.right.layout).some((id) =>
      windowStore.tabGroups[id]?.tabs.some((t) => t.id === `session:${sid()}`),
    );
  const links = noteLinks(true);
  let root: HTMLDivElement | undefined;
  // While the session covers the centre, a peek floats at the right edge.
  // The transcript makes room for it instead of running under it.
  const peekRoom = () => {
    if (!expanded()) return 0;
    const peekWin = windowStore.floatingWindows.find(
      (w) => !w.isMinimized && windowStore.tabGroups[w.tabGroupId]?.tabs.some((t) => t.id.startsWith('peek:')),
    );
    // Room only where the transcript keeps a readable column (about 34rem)
    // beside the peek; a narrower window lets the peek float over it.
    if (!peekWin || window.innerWidth - peekWin.width < 900) return 0;
    // Measured from this view's own right edge: the shell's gaps and the
    // peek's inset both sit between that edge and the window edge.
    const right = root?.getBoundingClientRect().right ?? window.innerWidth;
    return Math.max(0, right - peekWin.x) + 24;
  };
  return (
    <SessionLayout
      ref={(el) => (root = el)}
      insetRight={peekRoom()}
      header={
        <SessionHeader
          title={s().title}
          color={s().color}
          ctx={s().ctx}
          pending={sid() === 's3' ? proposalPending().length : 0}
          expanded={expanded()}
          onReview={() => openChanges(sid())}
          onToggleExpand={inRail() ? () => withTransition(() => windowActions.toggleEdgeExpanded('right')) : undefined}
        />
      }
      transcript={
        <Transcript
          blocks={blocks(state.transcripts[sid()] ?? [])}
          links={links}
          tools={mockToolHandlers}
          turn={{
            onEdit: (text) => {
              setState('drafts', sid(), text);
              focusComposer();
            },
            // The mockup has no model, so a regenerate does nothing. The real
            // app sends the last user message again (`chat.sendMessage`).
            onRegenerate: () => {},
          }}
        />
      }
      footer={<SessionFooterContainer sid={sid()} />}
    />
  );
};
