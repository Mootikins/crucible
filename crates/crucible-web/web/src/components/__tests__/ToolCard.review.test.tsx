import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
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
vi.mock('@/lib/api', () => ({
  getFileContent: vi.fn(async () => ''),
  subscribeToEvents: () => () => {},
}));
vi.mock('@/lib/file-actions', () => ({ openFileWithDiff: vi.fn() }));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: vi.fn() },
}));

const [sessionId, setSessionId] = createSignal<string | undefined>('s1');
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({ sessionId }),
}));

const listReviewHunks = vi.fn();
const setHunkState = vi.fn(async () => ({ hunk_id: 'h1', state: 'accepted' as const }));
vi.mock('@/lib/review-api', () => ({
  listReviewHunks: (...a: unknown[]) => listReviewHunks(...a),
  setHunkState: (...a: unknown[]) => setHunkState(...(a as [])),
  addReviewComment: vi.fn(),
  resolveReviewComment: vi.fn(),
}));

const { ToolCard } = await import('../ToolCard');
const { __resetReviewStore, reviewActions, revealedToolCall, toolCallLabel } = await import(
  '@/lib/review-store'
);

/** An Edit call, so `extractDiffFromToolCall` produces a diff and the card is
 *  eligible to carry review affordances at all. */
function editCall(over: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    callId: 'call-1',
    name: 'Edit',
    args: JSON.stringify({ file_path: '/repo/src/a.rs', old_string: 'a', new_string: 'b' }),
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

const seed = async (hunks: ComposedHunk[]) => {
  listReviewHunks.mockImplementation(async () => ({
    session_id: 's1',
    hunks: structuredClone(hunks),
    comments: [],
  }));
  await reviewActions.refresh('s1');
};

beforeEach(() => {
  setSessionId('s1');
  setHunkState.mockReset();
  setHunkState.mockResolvedValue({ hunk_id: 'h1', state: 'accepted' });
});

afterEach(() => {
  cleanup();
  __resetReviewStore();
  vi.clearAllMocks();
});

/** Cards start collapsed; the diff toolbar is inside. */
const expand = () => fireEvent.click(screen.getByRole('button', { expanded: false }));

describe('ToolCard — review attribution', () => {
  it('stamps the daemon call id so the editor gutter can scroll to it', () => {
    const { container } = render(() => <ToolCard toolCall={editCall()} />);
    expect(container.querySelector('[data-tool-call-id="call-1"]')).toBeTruthy();
  });

  it('publishes the tool name for surfaces that only have ids', () => {
    render(() => <ToolCard toolCall={editCall()} />);
    // The Changes panel and the editor gutter render outside ChatProvider and
    // have no other source for this.
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

  it('highlights the card the gutter chip pointed at', async () => {
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

  it('a call whose hunk is still live is NOT superseded', async () => {
    await seed([hunk()]);
    render(() => <ToolCard toolCall={editCall()} />);
    await waitFor(() => expect(screen.queryByTestId('tool-superseded')).toBeNull());
  });

  it('a call that proposed no edit is never called superseded', async () => {
    await seed([]);
    render(() => <ToolCard toolCall={editCall({ name: 'read_file', args: '{}' })} />);
    // It produced no hunks because it wrote nothing, not because it was
    // overwritten.
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
  });
});

describe('ToolCard — accept / reject', () => {
  it('offers one control pair per live composed hunk', async () => {
    await seed([hunk({ id: 'h1' }), hunk({ id: 'h2', current_range: { start: 30, end: 31 } })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('tool-hunk-h1')).toBeInTheDocument());
    expect(screen.getByTestId('tool-hunk-h2')).toBeInTheDocument();
  });

  it('accepting from the card names the composed hunk, not the tool call', async () => {
    await seed([hunk({ id: 'h1' })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    fireEvent.click(await waitFor(() => screen.getByTestId('tool-accept-h1')));
    // The action unit is the composed hunk — rejecting a call's own
    // contribution after later edits touched the same lines is a three-way
    // merge that fails exactly when it matters.
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'accepted'));
  });

  it('rejecting from the card reverts the composed hunk', async () => {
    await seed([hunk({ id: 'h1' })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    fireEvent.click(await waitFor(() => screen.getByTestId('tool-reject-h1')));
    await waitFor(() => expect(setHunkState).toHaveBeenCalledWith('s1', 'h1', 'rejected'));
  });

  it('an accepted hunk keeps its reject, loses its accept', async () => {
    await seed([hunk({ id: 'h1', state: 'accepted' })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('tool-reject-h1')).toBeInTheDocument());
    expect(screen.queryByTestId('tool-accept-h1')).toBeNull();
  });

  it('a superseded call has no accept/reject at all — the action unit is gone', async () => {
    await seed([hunk({ tool_call_ids: ['call-2'] })]);
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('tool-superseded')).toBeInTheDocument());
    expect(screen.queryByTestId('tool-accept-h1')).toBeNull();
    expect(screen.queryByTestId('tool-reject-h1')).toBeNull();
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
