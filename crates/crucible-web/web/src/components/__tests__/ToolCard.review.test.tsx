import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import type { ToolCallDisplay } from '@/lib/types';

// Shiki-free diff stubs, as in ToolCard.test.tsx — the real path lives in
// ToolCard.integration.test.tsx and the two cannot be merged.
vi.mock('../DiffViewer', () => ({
  DiffViewer: () => <div data-testid="diff-viewer" />,
}));
vi.mock('../MultiEditDiff', () => ({
  MultiEditDiff: () => <div data-testid="multi-edit-diff" />,
}));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: vi.fn() },
}));

const { ToolCard } = await import('../ToolCard');

/** An Edit call whose daemon-recorded diff proposes `a` → `b`. */
function editCall(over: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    callId: 'call-1',
    name: 'Edit',
    args: JSON.stringify({ file_path: '/repo/src/a.rs', old_string: 'a', new_string: 'b' }),
    display: { kind: 'file_edit', tool: 'Edit', diffs: [{ path: '/repo/src/a.rs', old_content: 'a', new_content: 'b' }] },
    status: 'complete',
    result: 'ok',
    ...over,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/** Cards start collapsed; the diff toolbar is inside. */
const expand = () => fireEvent.click(screen.getByRole('button', { expanded: false }));

describe('ToolCard — the attribution record', () => {
  it('stamps the daemon call id', () => {
    const { container } = render(() => <ToolCard toolCall={editCall()} />);
    expect(container.querySelector('[data-tool-call-id="call-1"]')).toBeTruthy();
  });

  it('a card with no daemon call id carries no id', () => {
    const { container } = render(() => <ToolCard toolCall={editCall({ callId: undefined })} />);
    // `toolCall.id` is a transcript id; using it would mis-link to another
    // call.
    expect(container.querySelector('[data-tool-call-id]')).toBeNull();
  });

  // The daemon no longer lists hunks, so the card shows what the call did and
  // no hunk chip or "superseded" claim.
  it('shows the call diff with no hunk chip and no superseded label', async () => {
    render(() => <ToolCard toolCall={editCall()} />);
    expand();
    await waitFor(() => expect(screen.getByTestId('diff-viewer')).toBeInTheDocument());
    expect(screen.queryByTestId('tool-superseded')).toBeNull();
    expect(document.querySelector('[data-testid^="tool-hunk-"]')).toBeNull();
  });
});
