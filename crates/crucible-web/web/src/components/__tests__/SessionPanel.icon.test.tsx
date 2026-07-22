import { describe, it, expect, vi } from 'vitest';
import { render } from '@solidjs/testing-library';
import type { Session } from '@/lib/types';
import { SessionPanel } from '../SessionPanel';
import { SessionFooter } from '../SessionFooter';

// The panel fetches kilns/branches on mount — keep it hermetic.
vi.mock('@/lib/api', () => ({
  listKilns: vi.fn().mockResolvedValue([]),
  scmBranches: vi.fn().mockRejectedValue(new Error('no repo')),
  searchSessions: vi.fn().mockResolvedValue([]),
}));

const session: Session = {
  id: 's1',
  session_type: 'chat',
  kiln: '/kiln',
  workspace: '/ws',
  connected_kilns: [],
  state: 'active',
  title: 'A session',
  agent_model: 'llama3.2',
  agent_mode: null,
  started_at: '',
  last_activity: null,
  event_count: 0,
  archived: false,
};

describe('SessionPanel icons — rendered DOM', () => {
  // Rendered without providers: the Safe context hooks fall back to inert
  // defaults, which is all these DOM-shape assertions need.
  it('new-session and add-project buttons render Lucide <svg>s, no glyph prefixes', () => {
    const { getByTestId, getByText } = render(() => <SessionPanel />);

    const newSession = getByTestId('new-session-button');
    expect(newSession.querySelector('svg')).toBeTruthy();
    expect(newSession.textContent?.trim()).toBe('New Session');

    const addProject = getByText('Add Project').closest('button')!;
    expect(addProject.querySelector('svg')).toBeTruthy();
    expect(addProject.textContent?.trim()).toBe('Add Project');
  });

  it('SessionFooter refresh button renders a RefreshCw <svg>, not a "↻" glyph', () => {
    const { container } = render(() => (
      <SessionFooter
        session={session}
        onPause={vi.fn()}
        onResume={vi.fn()}
        onRefresh={vi.fn()}
      />
    ));

    const buttons = Array.from(container.querySelectorAll('button'));
    const refreshBtn = buttons.find((b) => b.querySelector('svg') && b.textContent?.trim() === '');
    expect(refreshBtn, 'refresh button with an svg and no text').toBeTruthy();
    expect(container.textContent ?? '').not.toContain('↻');
  });
});
