import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { createEffect, createSignal } from 'solid-js';

// Two panes, two sessions, one tree. The transcript of a session belongs to
// the SESSION, not to whichever pane mounted first: a second pane that binds
// to a session another pane is already showing must see the transcript that
// session built, and neither session's events may land in the other's view.

// No `vi.mock('@/lib/api')`. The REAL subscribeToEvents runs: the
// FakeEventSource below answers it, one source per session, so each pane's
// stream is its session's alone. Each session's record and transcript answer
// under their own id, so a pane that binds reads the session it named.
const sessionOf = (id: string) => ({
  session_id: id, type: 'chat', title: 'T', state: 'active', kilns: ['k'], workspace: '/w',
  agent: { model: null }, started_at: '', event_count: 0, archived: false,
});
const historyOf = (id: string) => ({ session_id: id, history: [], total_events: 0 });

import { resetTranscriptsForTests } from '../transcriptStore';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { resetSseForTests } from '@/lib/query/sse';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

let env: TestQueryEnv;
beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/session-a': () => sessionOf('session-a'),
    'GET /api/session/session-b': () => sessionOf('session-b'),
    'GET /api/session/session-a/history': () => historyOf('session-a'),
    'GET /api/session/session-b/history': () => historyOf('session-b'),
  });
});

afterEach(() => {
  env?.restore();
  resetSseForTests();
  // The transcript store is a module singleton keyed by session; forget it
  // so one case's session cannot answer the next one.
  resetTranscriptsForTests();
});

/**
 * Reports this pane's transcript, so a test can prove the pane still hears.
 * Pushes only when the CONTENT moves: a rebind's empty-history fold rebuilds
 * the message array's identity, and an identity-only change is not a new
 * transcript.
 */
function TokenCollector(props: { onMessages: (m: { content: string }[]) => void }) {
  const { messages } = useChat();
  let last = '';
  createEffect(() => {
    const joined = messages()
      .filter((m) => m.content !== '')
      .map((m) => m.content)
      .join('|');
    if (joined !== '' && joined !== last) {
      last = joined;
      props.onMessages(joined.split('|').map((content) => ({ content })));
    }
  });
  return <span />;
}

/** Hands the pane's own context to the test, whatever session it binds to. */
function ContextProbe(props: { onContext: (ctx: ChatContextValue) => void }) {
  props.onContext(useChat());
  return <span />;
}

describe('two providers, two sessions', () => {
  it('keeps both transcripts independent, and a rebinding pane joins the one it adopts', async () => {
    const aSeen: string[] = [];
    const bSeen: string[] = [];
    // The second pane starts on session B and later rebinds onto session A —
    // the split-pane adoption of a session another pane already shows.
    const [paneTwoSession, rebindPaneTwo] = createSignal('session-b');
    let paneTwo!: ChatContextValue;

    render(() => (
      <>
        <ChatProvider sessionId="session-a">
          <TokenCollector onMessages={(m) => aSeen.push(...m.map((x) => x.content))} />
        </ChatProvider>
        <ChatProvider sessionId={paneTwoSession()}>
          <TokenCollector onMessages={(m) => bSeen.push(...m.map((x) => x.content))} />
          <ContextProbe onContext={(ctx) => { paneTwo = ctx; }} />
        </ChatProvider>
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2));
    const [srcA, srcB] = FakeEventSource.instances;

    srcA!.emit('message_complete', { type: 'message_complete', id: 'turn-a', content: 'answer for A' });
    srcB!.emit('message_complete', { type: 'message_complete', id: 'turn-b', content: 'answer for B' });

    // One event to each: neither session's transcript may leak into the other.
    await waitFor(() => expect(aSeen).toEqual(['answer for A']));
    await waitFor(() => expect(bSeen).toEqual(['answer for B']));

    rebindPaneTwo('session-a');

    // The pane that adopted session A shows the transcript session A already
    // built — the streamed answer, not a blank fold that waits for a refetch.
    await waitFor(() => expect(paneTwo.messages().map((m) => m.content)).toContain('answer for A'));
    // And the pane that never moved still holds its own transcript.
    expect(aSeen).toEqual(['answer for A']);
  });

  it('folds a replayed one-shot event once', async () => {
    let ctx!: ChatContextValue;
    const Probe = () => { ctx = useChat(); return null; };
    render(() => (
      <ChatProvider sessionId="session-a">
        <Probe />
      </ChatProvider>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const toolCall = { type: 'tool_call', id: 'call-1', title: 'update_note', arguments: {} };
    FakeEventSource.instances[0]!.emit('tool_call', toolCall);
    // The same event again — a replay must not draw the tool a second time.
    FakeEventSource.instances[0]!.emit('tool_call', toolCall);

    await waitFor(() => expect(ctx.messages().filter((m) => m.role === 'tool')).toHaveLength(1));
    expect(ctx.messages().filter((m) => m.role === 'tool')).toHaveLength(1);
  });
});
