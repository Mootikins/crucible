import { describe, it, expect, vi } from 'vitest';
import { render } from '@solidjs/testing-library';
import type { Session, KilnInfo } from '@/lib/types';
import { ProjectSection } from '../ProjectSection';
import { SessionSection } from '../SessionSection';
import { SessionFooter } from '../SessionFooter';

// The old test concatenated the SOURCE of SessionPanel + its sub-sections and
// grepped for icon identifiers / the ABSENCE of "↻" and "+ Add Project". That
// passes even if nothing renders. Here we render the three sub-sections that
// actually own the affected buttons and assert on the emitted DOM: the add
// buttons carry a Lucide <svg> (Plus) and never a literal "+" prefix, and the
// refresh button carries an <svg> (RefreshCw) and never a "↻" glyph.

// SessionSection carries required props; a minimal set is enough to render.
const sessionSectionProps = {
  kilns: [] as KilnInfo[],
  selectedKiln: '/kiln',
  onKilnSelect: vi.fn(),
  sessionFilter: 'active' as const,
  onSessionFilterChange: vi.fn(),
  searchQuery: '',
  isSearching: false,
  onSearchInput: vi.fn(),
  onClearSearch: vi.fn(),
  setSearchInputRef: vi.fn(),
  displayedSessions: [] as Session[],
  currentSession: undefined,
  onSelectSession: vi.fn(),
  onArchiveSession: vi.fn(),
  onDeleteSession: vi.fn(),
  onCreateSession: vi.fn(),
  isLoading: false,
  hasProviders: true,
  providersLoaded: true,
};

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
  event_count: 0,
};

describe('SessionPanel icons — rendered DOM', () => {
  it('ProjectSection add button renders a Plus <svg>, not a "+" prefix', () => {
    const { getByText } = render(() => (
      <ProjectSection
        projects={[]}
        currentProject={undefined}
        onSelectProject={vi.fn()}
        onRegisterProject={vi.fn(async () => {})}
      />
    ));

    const label = getByText('Add Project');
    const button = label.closest('button');
    expect(button).toBeTruthy();
    expect(button!.querySelector('svg')).toBeTruthy();
    // The visible label is exactly "Add Project" — no "+ " text prefix.
    expect(button!.textContent).not.toContain('+ Add Project');
    expect(button!.textContent?.trim()).toBe('Add Project');
  });

  it('SessionSection new-session button renders a Plus <svg>, not a "+" prefix', () => {
    const { getByTestId } = render(() => <SessionSection {...sessionSectionProps} />);

    const button = getByTestId('new-session-button');
    expect(button.querySelector('svg')).toBeTruthy();
    expect(button.textContent).not.toContain('+ New Session');
    expect(button.textContent?.trim()).toBe('New Session');
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

    // The refresh button is the only icon-only button in the footer.
    const buttons = Array.from(container.querySelectorAll('button'));
    const refreshBtn = buttons.find((b) => b.querySelector('svg') && b.textContent?.trim() === '');
    expect(refreshBtn, 'refresh button with an svg and no text').toBeTruthy();
    expect(container.textContent ?? '').not.toContain('↻');
  });

  it('renders at least two Plus add-buttons across the project + session sections', () => {
    const project = render(() => (
      <ProjectSection
        projects={[]}
        currentProject={undefined}
        onSelectProject={vi.fn()}
        onRegisterProject={vi.fn(async () => {})}
      />
    ));
    const sessions = render(() => <SessionSection {...sessionSectionProps} />);

    const addProject = project.getByText('Add Project').closest('button')!;
    const newSession = sessions.getByTestId('new-session-button');
    expect(addProject.querySelector('svg')).toBeTruthy();
    expect(newSession.querySelector('svg')).toBeTruthy();
  });
});
