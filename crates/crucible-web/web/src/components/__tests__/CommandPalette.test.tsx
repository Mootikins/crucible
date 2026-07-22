import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { CommandPalette, parseOmniQuery, type PaletteCommand } from '../CommandPalette';
import { statusBarActions } from '@/stores/statusBarStore';
import type { NoteEntry } from '@/lib/types';

const PLACEHOLDER = 'Go anywhere… ( > command · [[ note )';

const mockNotes: NoteEntry[] = [
  {
    name: 'Architecture',
    path: '/kiln/Architecture.md',
    title: 'Architecture',
    tags: ['meta'],
    updated_at: '2026-07-10T10:00:00Z',
  },
];

vi.mock('@/lib/api', () => ({
  listNotes: vi.fn(() => Promise.resolve(mockNotes)),
}));

// Kobalte's Dialog renders into a Portal appended to document.body. Even
// though solid-testing-library auto-cleans the render container, the
// portaled dialog content can persist across tests (sentinel focus-trap
// nodes, leftover Kobalte presence wrappers). Wipe the body before each
// test so `screen.*` queries see only the current test's portal.
beforeEach(() => {
  document.body.innerHTML = '';
  statusBarActions.setKilnPath('/kilns/helios');
  statusBarActions.setActiveSessionTitle(null);
});

function cmd(overrides: Partial<PaletteCommand> = {}): PaletteCommand {
  return {
    id: overrides.id ?? 'cmd-1',
    label: overrides.label ?? 'Do Something',
    category: overrides.category ?? 'Chat',
    action: overrides.action ?? vi.fn(),
    description: overrides.description,
    shortcut: overrides.shortcut,
    keywords: overrides.keywords,
  };
}

function getInput(): HTMLInputElement {
  return screen.getByPlaceholderText(PLACEHOLDER) as HTMLInputElement;
}

describe('parseOmniQuery — prefix routing', () => {
  it('routes > to commands', () => {
    expect(parseOmniQuery('>clear')).toEqual({ kinds: ['CMD'], query: 'clear' });
  });
  it('routes [[ to notes', () => {
    expect(parseOmniQuery('[[arch')).toEqual({ kinds: ['NOTE'], query: 'arch' });
  });
  it('leaves plain queries unscoped', () => {
    expect(parseOmniQuery('  inbox ')).toEqual({ kinds: null, query: 'inbox' });
  });
});

describe('CommandPalette — open / closed', () => {
  it('renders nothing visible when open is false', () => {
    render(() => (
      <CommandPalette open={false} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(screen.queryByPlaceholderText(PLACEHOLDER)).not.toBeInTheDocument();
  });

  it('renders the omnibox input when open is true', () => {
    render(() => (
      <CommandPalette open={true} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(getInput()).toBeInTheDocument();
  });
});

describe('CommandPalette — omnibox sections', () => {
  it('always offers the GO surfaces', () => {
    render(() => (
      <CommandPalette open={true} commands={[]} onOpenChange={() => {}} />
    ));
    // No Home surface — the landing page was removed.
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
    expect(screen.getByText('Inbox')).toBeInTheDocument();
    expect(screen.getByText('Editor (✎ Edit mode)')).toBeInTheDocument();
    expect(screen.getByText('Session (◆ Session mode)')).toBeInTheDocument();
  });

  it('renders provided commands with a CMD badge', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'a', label: 'Run Tests' })]}
        onOpenChange={() => {}}
      />
    ));
    expect(screen.getByText('Run Tests')).toBeInTheDocument();
    expect(screen.getAllByText('CMD').length).toBeGreaterThan(0);
  });

  it('lists kiln notes once loaded', async () => {
    render(() => (
      <CommandPalette open={true} commands={[]} onOpenChange={() => {}} />
    ));
    await waitFor(() => {
      expect(screen.getByText('Architecture')).toBeInTheDocument();
    });
    expect(screen.getAllByText('NOTE').length).toBe(1);
  });
});

describe('CommandPalette — filtering & prefixes', () => {
  const commands = [
    cmd({ id: 'a', label: 'Compile Project', keywords: ['build', 'make'] }),
    cmd({ id: 'b', label: 'Switch Workspace' }),
  ];

  it('filters by case-insensitive substring across label and keywords', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(), { target: { value: 'build' } });
    expect(screen.getByText('Compile Project')).toBeInTheDocument();
    expect(screen.queryByText('Switch Workspace')).not.toBeInTheDocument();
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
  });

  it('fuzzy: subsequence queries match (WS-304 fuzzy matching)', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'cc', label: 'Clear Chat' }), cmd({ id: 'sw', label: 'Switch Workspace' })]}
        onOpenChange={() => {}}
      />
    ));
    // "clch" is not a substring of anything but is a subsequence of "Clear Chat".
    fireEvent.input(getInput(), { target: { value: 'clch' } });
    expect(screen.getByText('Clear Chat')).toBeInTheDocument();
    expect(screen.queryByText('Switch Workspace')).not.toBeInTheDocument();
  });

  it('fuzzy: label matches rank above keyword-only matches', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[
          cmd({ id: 'kw', label: 'Toggle Theme', keywords: ['export'] }),
          cmd({ id: 'lbl', label: 'Export Session' }),
        ]}
        onOpenChange={() => {}}
      />
    ));
    fireEvent.input(getInput(), { target: { value: 'export' } });
    const labels = screen
      .getAllByText(/Export Session|Toggle Theme/)
      .map((el) => el.textContent);
    expect(labels[0]).toBe('Export Session');
    expect(labels[1]).toBe('Toggle Theme');
  });

  it('matches description text too (manual filtering, not cmdk)', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'x', label: 'Toggle Mode', description: 'flip between plan and normal' })]}
        onOpenChange={() => {}}
      />
    ));
    fireEvent.input(getInput(), { target: { value: 'plan' } });
    expect(screen.getByText('Toggle Mode')).toBeInTheDocument();
  });

  it('> scopes to commands only', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(), { target: { value: '>' } });
    expect(screen.getByText('Compile Project')).toBeInTheDocument();
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
  });

  it('[[ scopes to notes only', async () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(), { target: { value: '[[arch' } });
    await waitFor(() => {
      expect(screen.getByText('Architecture')).toBeInTheDocument();
    });
    expect(screen.queryByText('Compile Project')).not.toBeInTheDocument();
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
  });

  it('shows the empty message when nothing matches', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(), { target: { value: 'zzzznomatch' } });
    expect(screen.getByText(/Nothing matches/)).toBeInTheDocument();
  });
});

describe('CommandPalette — selection', () => {
  it('invokes the action and closes the palette when an item is selected', () => {
    const action = vi.fn();
    const onOpenChange = vi.fn();
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'a', label: 'Trigger Me', action })]}
        onOpenChange={onOpenChange}
      />
    ));
    const labelEl = screen.getByText('Trigger Me');
    const itemEl = labelEl.closest('[cmdk-item]') as HTMLElement;
    expect(itemEl).not.toBeNull();
    fireEvent.click(itemEl);

    expect(action).toHaveBeenCalledTimes(1);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});

describe('CommandPalette — extras', () => {
  it('renders shortcut as a <kbd>', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ label: 'Save', shortcut: '⌘S' })]}
        onOpenChange={() => {}}
      />
    ));
    const kbd = screen.getByText('⌘S');
    expect(kbd.tagName.toLowerCase()).toBe('kbd');
  });

  it('renders description when present', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ label: 'Compact', description: 'Squash conversation history' })]}
        onOpenChange={() => {}}
      />
    ));
    expect(screen.getByText('Squash conversation history')).toBeInTheDocument();
  });

  it('shows the prefix hint footer', () => {
    render(() => (
      <CommandPalette open={true} commands={[]} onOpenChange={() => {}} />
    ));
    expect(screen.getByText('command')).toBeInTheDocument();
    expect(screen.getByText('note')).toBeInTheDocument();
  });
});

describe('CommandPalette — query reset on close', () => {
  it('clears the query when the palette is closed', () => {
    const [open, setOpen] = createSignal(true);
    render(() => (
      <CommandPalette
        open={open()}
        commands={[cmd({ id: 'a', label: 'AAA' })]}
        onOpenChange={setOpen}
      />
    ));

    fireEvent.input(getInput(), { target: { value: 'foo' } });
    expect(getInput().value).toBe('foo');

    setOpen(false);
    setOpen(true);

    expect(getInput().value).toBe('');
  });

  it('opens seeded with initialQuery — the Ctrl+O note switcher lands in [[ mode', async () => {
    const [open, setOpen] = createSignal(false);
    render(() => (
      <CommandPalette
        open={open()}
        commands={[cmd({ id: 'a', label: 'AAA' })]}
        initialQuery="[["
        onOpenChange={setOpen}
      />
    ));

    setOpen(true);

    expect(getInput().value).toBe('[[');
    // Note-scoped: commands are filtered out, notes remain.
    await waitFor(() => {
      expect(screen.getByText('Architecture')).toBeInTheDocument();
    });
    expect(screen.queryByText('AAA')).not.toBeInTheDocument();
  });
});
