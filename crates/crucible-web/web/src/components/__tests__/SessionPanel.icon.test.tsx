import { describe, it, expect, vi } from 'vitest';
import { render } from '@solidjs/testing-library';
import { SessionPanel } from '../SessionPanel';

// The panel fetches kilns/branches on mount — keep it hermetic.
vi.mock('@/lib/api', () => ({
  listKilns: vi.fn().mockResolvedValue([]),
  scmBranches: vi.fn().mockRejectedValue(new Error('no repo')),
  scmClone: vi.fn(),
  isGitRepoUrl: (s: string) => s.startsWith('https://'),
  searchSessions: vi.fn().mockResolvedValue([]),
}));

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
});
