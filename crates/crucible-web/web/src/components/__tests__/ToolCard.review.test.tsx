import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import { createRoot, createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource } from '@/test-utils/sse';
import type { ToolCallDisplay } from '@/lib/types';
import type { ComposedHunk } from '@/lib/review-types';

// Shiki-free diff stubs, as in ToolCard.test.tsx — the real path lives in
// ToolCard.integration.test.tsx and the two cannot be merged.
vi.mock('../DiffViewer', () => ({
  DiffViewer: () => <div data-testid="diff-viewer" />,
}));
vi.mock('../MultiEditDiff', () => ({
  MultiEditDiff: () => <div data-testid="multi-edit-diff" />,
}));
// No `vi.mock('@/lib/api')`: the reveal path reads the file through the
// real `getFileContent` against the `GET /api/kiln/file` route below, and the
// review stream opens through the real `subscribeToEvents` onto the
// `FakeEventSource` of `beforeEach`.
vi.mock('@/lib/file-actions', () => ({ openFileWithDiff: vi.fn() }));
// The shell is decided once at page load; the test stages it before a render.
const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: vi.fn() },
}));

const [sessionId, setSessionId] = createSignal<string | undefined>('s1');
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({ sessionId }),
}));

const listReviewHunks = vi.fn();
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  addReviewComment: vi.fn(),
  resolveReviewComment: vi.fn(),
}));

const { ToolCard } = await import('../ToolCard');
const {
  __resetReviewStore,
  reviewActions,
  reviewStore,
  revealedToolCall,
  toolCallLabel,
  useReviewSession,
} = await import('@/lib/review-store');

/** An Edit call whose daemon-recorded diff proposes `a` → `b`, so the card is
 *  eligible to carry review affordances at all. */
function editCall(over: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    callId: 'call-1',
    name: 'Edit',
    args: JSON.stringify({ file_path: '/repo/src/a.rs', old_string: 'a', new_string: 'b' }),
    diffs: [{ path: '/repo/src/a.rs', old_content: 'a', new_content: 'b' }],
    status: 'complete',
    result: 'ok',
    ...over,
  };
}

function hunk(over: Partial<ComposedHunk> = {}): ComposedHunk {
  return {
    id: 'h1',
    root: '/repo',
    path: 'src/a.rs',
    base_range: { start: 4, end: 6 },
    current_range: { start: 4, end: 6 },
    before_content: 'a\n',
    after_content: 'b\n',
    tool_call_ids: ['call-1'],
    state: 'unreviewed',
    reapplied: false,
    ...over,
  };
}

let env: TestQueryEnv;
/** The panel standing in for whatever binds this session on screen. */
let unbind: (() => void) | null = null;

/**
 * Put a composed diff in front of the card.
 *
 * The listing is a cache entry now, and a slot is filled by the BINDING that
 * observes it — so this binds the session once, the way a panel does, and then
 * marks the listing wrong so the new answer lands.
 */
const seed = async (hunks: ComposedHunk[]) => {
  listReviewHunks.mockImplementation(async () => ({
    session_id: 's1',
    hunks: structuredClone(hunks),
    comments: [],
  }));
  if (!unbind) {
    unbind = createRoot((dispose) => {
      useReviewSession(() => 's1');
      return dispose;
    });
  }
  await reviewActions.refresh('s1');
  await waitFor(() => {
    expect(reviewStore.session('s1').loaded).toBe(true);
    expect(reviewStore.session('s1').hunks.map((h) => h.id)).toEqual(hunks.map((h) => h.id));
  });
};

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/kiln/file': () => ({ content: '' }) });
  setSessionId('s1');
});

afterEach(() => {
  cleanup();
  unbind?.();
  unbind = null;
  __resetReviewStore();
  env.restore();
  device.compact = false;
  vi.clearAllMocks();
});

/** Cards start collapsed; the diff toolbar is inside. */
const expand = () => fireEvent.click(screen.getByRole('button', { expanded: false }));

describe('ToolCard — review attribution', () => {
  it('stamps the daemon call id so a reveal can scroll to it', () => {
    const { container } = render(() => <ToolCard toolCall={editCall()} />);
    expect(container.querySelector('[data-tool-call-id="call-1"]')).toBeTruthy();
  });

  it('publishes the tool name for surfaces that only have ids', () => {
    render(() => <ToolCard toolCall={editCall()} />);
    // The Changes panel renders outside ChatProvider and has
    // no other source for this.
    expect(toolCallLabel('call-1')).toBe('Edit');
  });

  it('a card with no daemon call id carries no attribution and no claim', async () => {
    await seed([]);
    const { container } = render(() => <ToolCard toolCall={editCall({ callId: undefined })} />);
    // `toolCall.id` is a transcript id; using it would mis-link to another
    // call's hunks, so an unidentified card degrades to showing nothing.
    expect(container.querySelector('[data-tool-call-id]')).toBeNull();
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });

  it('highlights the card that a reveal names', async () => {
    const { container } = render(() => <ToolCard toolCall={editCall()} />);
    reviewActions.revealToolCall('call-1');
    expect(revealedToolCall()).toBe('call-1');
    await waitFor(() => expect(container.firstElementChild!.className).toContain('ring-attention'));
  });
});

describe('ToolCard — superseded', () => {
  it('says nothing until the ledger has actually been read', () => {
    // No refresh: the composed diff is unknown. Claiming "superseded" here
    // would announce that an edit was thrown away on the strength of a request
    // that never happened.
    render(() => <ToolCard toolCall={editCall()} />);
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });

  it('marks the call superseded when nothing it wrote survives', async () => {
    await seed([hunk({ tool_call_ids: ['call-2'] })]);
    render(() => <ToolCard toolCall={editCall()} />);
    await waitFor(() => expect(screen.getByTestId('tool-superseded')).toBeInTheDocument());
  });

  it('a turn-scoped listing never marks an earlier call superseded', async () => {
    // Under the turn scope the store holds only the current turn's hunks, so
    // an earlier call's absence says nothing about whether its edit survived.
    await seed([]);
    await reviewActions.setScope('s1', 'turn');
    render(() => <ToolCard toolCall={editCall()} />);
    await waitFor(() => expect(reviewActions).toBeDefined());
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });

  it('a call whose hunk is still live is NOT superseded', async () => {
    await seed([hunk()]);
    render(() => <ToolCard toolCall={editCall()} />);
    await waitFor(() => expect(screen.queryByTestId('tool-superseded')).toBeNull());
  });

  it('a call that proposed no edit is never called superseded', async () => {
    await seed([]);
    render(() => <ToolCard toolCall={editCall({ name: 'read_file', args: '{}', diffs: [] })} />);
    // It produced no hunks because it wrote nothing, not because it was
    // overwritten.
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });
});

describe('ToolCard — live hunks', () => {
  // The daemon no longer accepts or reverts a hunk, so a chip names the lines
  // and offers no decision.
  it('shows one chip per live composed hunk, with no decision on it', async () => {
    await seed([hunk({ id: 'h1' }), hunk({ id: 'h2', current_range: { start: 30, end: 31 } })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('tool-hunk-h1')).toBeInTheDocument());
    expect(screen.getByTestId('tool-hunk-h2')).toBeInTheDocument();
    expect(screen.queryByTestId('tool-accept-h1')).toBeNull();
    expect(screen.queryByTestId('tool-reject-h1')).toBeNull();
  });

  it('a superseded call has no hunk chip — the action unit is gone', async () => {
    await seed([hunk({ tool_call_ids: ['call-2'] })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('tool-superseded')).toBeInTheDocument());
    expect(screen.queryByTestId('tool-hunk-h1')).toBeNull();
    // The card still shows what the call did — it is an index entry, not a
    // pending action.
    expect(screen.getByTestId('diff-viewer')).toBeInTheDocument();
  });

  it('shows nothing review-shaped outside a chat session', async () => {
    await seed([hunk()]);
    setSessionId(undefined);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('diff-viewer')).toBeInTheDocument());
    expect(screen.queryByTestId('tool-hunk-h1')).toBeNull();
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });
});
