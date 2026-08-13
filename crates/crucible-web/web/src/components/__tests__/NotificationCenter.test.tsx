import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import type { Notification } from '@/lib/types';

// Mock the store. The component reads `notificationStore.notifications`
// and calls `notificationActions.{dismiss,clearAll,markAllRead}`. We don't
// need reactivity inside a single test — each `render(() => <NC ... />)`
// reads the current `mockState` value at first render.
type MockState = { notifications: Notification[] };
const mockState: MockState = { notifications: [] };

// Mocks are spies only — we deliberately do NOT mutate state inside them.
// The component's effect-driven markAllRead would otherwise hide the
// per-item dismiss button before any test could click it. Behavior of the
// real store is covered in its own (future) unit test.
const dismissMock = vi.fn();
const clearAllMock = vi.fn();
const markAllReadMock = vi.fn();

vi.mock('@/stores/notificationStore', () => ({
  notificationStore: {
    get notifications() {
      return mockState.notifications;
    },
  },
  notificationActions: {
    dismiss: (id: string) => dismissMock(id),
    clearAll: () => clearAllMock(),
    markAllRead: () => markAllReadMock(),
  },
}));

// Import after mocks.
import { NotificationCenter } from '../NotificationCenter';

function makeNotif(overrides: Partial<Notification> = {}): Notification {
  return {
    id: `n-${Math.random()}`,
    type: 'info',
    message: 'hello',
    timestamp: Date.now(),
    dismissed: false,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockState.notifications = [];
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('NotificationCenter — open / close', () => {
  it('renders nothing when closed', () => {
    render(() => <NotificationCenter open={false} onClose={() => {}} />);
    expect(screen.queryByText('Notifications')).not.toBeInTheDocument();
  });

  it('renders the drawer header when open', () => {
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('Notifications')).toBeInTheDocument();
  });

  it('marks notifications as read when opened', () => {
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(markAllReadMock).toHaveBeenCalledTimes(1);
  });

  it('does not call markAllRead while closed', () => {
    render(() => <NotificationCenter open={false} onClose={() => {}} />);
    expect(markAllReadMock).not.toHaveBeenCalled();
  });
});

describe('NotificationCenter — empty state', () => {
  it('shows the empty placeholder when no notifications exist', () => {
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('No notifications')).toBeInTheDocument();
  });

  it('hides the Clear All button when there are no notifications', () => {
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.queryByText('Clear All')).not.toBeInTheDocument();
  });
});

describe('NotificationCenter — list rendering', () => {
  it('renders a single notification row with its message', () => {
    mockState.notifications = [makeNotif({ id: 'n1', message: 'one message' })];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('one message')).toBeInTheDocument();
  });

  it('shows the count badge in the header', () => {
    mockState.notifications = [
      makeNotif({ id: 'a' }),
      makeNotif({ id: 'b' }),
      makeNotif({ id: 'c' }),
    ];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    // Header count: scope to the badge so we don't match "3 hours" later
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  it('shows the Clear All button when notifications exist', () => {
    mockState.notifications = [makeNotif()];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('Clear All')).toBeInTheDocument();
  });

  it('renders the correct icon for each notification type', () => {
    mockState.notifications = [
      makeNotif({ id: 'i', type: 'info', message: 'info' }),
      makeNotif({ id: 's', type: 'success', message: 'success' }),
      makeNotif({ id: 'w', type: 'warning', message: 'warning' }),
      makeNotif({ id: 'e', type: 'error', message: 'error' }),
    ];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('ℹ')).toBeInTheDocument();
    expect(screen.getByText('✓')).toBeInTheDocument();
    expect(screen.getByText('⚠')).toBeInTheDocument();
    expect(screen.getByText('✕')).toBeInTheDocument();
  });
});

describe('NotificationCenter — time grouping', () => {
  it('groups by Today / Yesterday / Older with correct ordering', () => {
    const now = Date.now();
    const DAY = 86_400_000;
    mockState.notifications = [
      makeNotif({ id: 'old', timestamp: now - 5 * DAY, message: 'old-msg' }),
      makeNotif({ id: 'today', timestamp: now - 1000, message: 'today-msg' }),
      makeNotif({ id: 'yest', timestamp: now - 1.5 * DAY, message: 'yest-msg' }),
    ];
    // Queried from `document.body`, not `render`'s container: the popout is
    // portaled so it can escape the right ribbon's `overflow-hidden`
    // ancestors, which puts it outside the container by construction.
    render(() => <NotificationCenter open={true} onClose={() => {}} />);

    const headers = Array.from(document.body.querySelectorAll('span.uppercase'))
      .map((el) => el.textContent?.trim())
      .filter((s): s is string => !!s && ['Today', 'Yesterday', 'Older'].includes(s));

    expect(headers).toEqual(['Today', 'Yesterday', 'Older']);
  });

  it('orders items within a group newest-first', () => {
    const now = Date.now();
    mockState.notifications = [
      makeNotif({ id: 'a', timestamp: now - 60_000, message: 'older-today' }),
      makeNotif({ id: 'b', timestamp: now - 1000, message: 'newer-today' }),
    ];
    // Portaled — see the note above.
    render(() => <NotificationCenter open={true} onClose={() => {}} />);

    const messages = Array.from(document.body.querySelectorAll('p'))
      .map((el) => el.textContent ?? '')
      .filter((s) => s === 'older-today' || s === 'newer-today');

    expect(messages).toEqual(['newer-today', 'older-today']);
  });
});

describe('NotificationCenter — actions', () => {
  it('calls dismiss when the per-item X is clicked', () => {
    mockState.notifications = [makeNotif({ id: 'to-dismiss', message: 'm' })];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    fireEvent.click(screen.getByLabelText('Dismiss'));
    expect(dismissMock).toHaveBeenCalledWith('to-dismiss');
  });

  it('hides the per-item X for already-dismissed notifications', () => {
    mockState.notifications = [
      makeNotif({ id: 'd', message: 'gone', dismissed: true }),
    ];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.queryByLabelText('Dismiss')).not.toBeInTheDocument();
  });

  it('calls clearAll when the header button is clicked', () => {
    mockState.notifications = [makeNotif()];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    fireEvent.click(screen.getByText('Clear All'));
    expect(clearAllMock).toHaveBeenCalledTimes(1);
  });

  it('calls onClose when the X in the header is clicked', () => {
    const onClose = vi.fn();
    render(() => <NotificationCenter open={true} onClose={onClose} />);
    fireEvent.click(screen.getByLabelText('Close notifications'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe('NotificationCenter — keyboard / backdrop', () => {
  it('calls onClose when Escape is pressed', () => {
    const onClose = vi.fn();
    render(() => <NotificationCenter open={true} onClose={onClose} />);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not respond to Escape when closed', () => {
    const onClose = vi.fn();
    render(() => <NotificationCenter open={false} onClose={onClose} />);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
  });

  it('calls onClose on an outside click (popout has no backdrop)', () => {
    const onClose = vi.fn();
    render(() => <NotificationCenter open={true} onClose={onClose} />);
    // The popout dismisses via a document-level listener when the press
    // lands outside its anchor parent.
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  /** A bell-shaped anchor at the bottom-right, like the right ribbon's. */
  function bellAt(x: number, y: number): HTMLElement {
    const el = document.createElement('button');
    document.body.appendChild(el);
    // jsdom reports an all-zero rect, so the geometry has to be supplied.
    vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
      x, y, left: x, top: y, width: 40, height: 36,
      right: x + 40, bottom: y + 36, toJSON: () => ({}),
    } as DOMRect);
    return el;
  }

  it('sits just above the bell rather than floating high', () => {
    // The bug: placement came from `placeFlyout`, which aligns a submenu's TOP
    // with its row and clamps using maxHeight — so a panel shorter than its cap
    // ended up 265px above a bell at the bottom of the rail. Anchoring the
    // BOTTOM pins it to the bell whatever the content height.
    vi.stubGlobal('innerWidth', 1920);
    vi.stubGlobal('innerHeight', 1080);
    const anchor = bellAt(1880, 1044);

    render(() => <NotificationCenter open={true} onClose={() => {}} anchor={anchor} />);
    const panel = document.body.querySelector<HTMLElement>('[data-testid="notification-popout"]')!;

    // 8px above the bell's top edge, expressed as a viewport-bottom offset.
    expect(panel.style.bottom).toBe(`${1080 - 1044 + 8}px`);
    // Flipped left of a rail that cannot hold 320px, and on screen.
    expect(parseInt(panel.style.left, 10)).toBeLessThan(1880);
    expect(parseInt(panel.style.left, 10)).toBeGreaterThanOrEqual(0);
    // Capped by the room above the bell, not by a fixed 480.
    expect(parseInt(panel.style.maxHeight, 10)).toBeLessThanOrEqual(1044 - 16);
  });

  it('does not call onClose when a click lands on the bell itself', () => {
    // Portaled, `panelRef.parentElement` is the portal container, so a single
    // containment check read a bell click as OUTSIDE — closing the popout just
    // as the bell's own handler reopened it.
    const onClose = vi.fn();
    const anchor = bellAt(1880, 1044);
    render(() => <NotificationCenter open={true} onClose={onClose} anchor={anchor} />);
    fireEvent.mouseDown(anchor);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not call onClose when a child of the popout is clicked', () => {
    const onClose = vi.fn();
    mockState.notifications = [makeNotif({ message: 'child-click-target' })];
    render(() => <NotificationCenter open={true} onClose={onClose} />);
    fireEvent.mouseDown(screen.getByText('child-click-target'));
    fireEvent.click(screen.getByText('child-click-target'));
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe('NotificationCenter — formatTime', () => {
  it('formats hours and minutes with zero-padding', () => {
    const ts = new Date(2026, 0, 1, 7, 5).getTime();
    mockState.notifications = [makeNotif({ id: 't', timestamp: ts, message: 'tstamp' })];
    render(() => <NotificationCenter open={true} onClose={() => {}} />);
    expect(screen.getByText('07:05')).toBeInTheDocument();
  });
});
