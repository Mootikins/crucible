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
    const { container } = render(() => (
      <NotificationCenter open={true} onClose={() => {}} />
    ));

    const headers = Array.from(container.querySelectorAll('span.uppercase'))
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
    const { container } = render(() => (
      <NotificationCenter open={true} onClose={() => {}} />
    ));

    const messages = Array.from(container.querySelectorAll('p'))
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

  it('calls onClose when the backdrop is clicked', () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <NotificationCenter open={true} onClose={onClose} />
    ));
    const backdrop = container.querySelector('.fixed.inset-0') as HTMLElement;
    expect(backdrop).not.toBeNull();
    // Dispatch a click whose target IS the backdrop element
    fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not call onClose when a child of the drawer is clicked', () => {
    const onClose = vi.fn();
    mockState.notifications = [makeNotif({ message: 'child-click-target' })];
    render(() => <NotificationCenter open={true} onClose={onClose} />);
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
