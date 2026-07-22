import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { CommandPalette, parseOmniQuery, type PaletteCommand } from '../CommandPalette';
import { statusBarActions } from '@/stores/statusBarStore';
import type { NoteEntry } from '@/lib/types';

const CMD_PLACEHOLDER = 'Run a command… ( [[ to open a note )';
const NOTE_PLACEHOLDER = 'Open note… ( > to run a command )';

const mockNotes: NoteEntry[] = [
  {
    name: 'Architecture',
    path: '/kiln/Architecture.md',
    title: 'Architecture',
    tags: ['meta'],
    updated_at: '2026-07-10T10:00:00Z',
  },
  {
    name: 'Roadmap',
    path: '/kiln/Meta/Roadmap.md',
    title: 'Roadmap',
    tags: [],
    updated_at: '2026-07-12T10:00:00Z',
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

function getInput(placeholder = CMD_PLACEHOLDER): HTMLInputElement {
  return screen.getByPlaceholderText(placeholder) as HTMLInputElement;
}

describe('parseOmniQuery — prefix routing', () => {
  it('routes > to commands from any mode', () => {
    expect(parseOmniQuery('>clear', 'notes')).toEqual({ kind: 'CMD', query: 'clear' });
  });
  it('routes [[ to notes from any mode', () => {
    expect(parseOmniQuery('[[arch', 'commands')).toEqual({ kind: 'NOTE', query: 'arch' });
  });
  it('plain queries scope to the open mode', () => {
    expect(parseOmniQuery('  inbox ', 'commands')).toEqual({ kind: 'CMD', query: 'inbox' });
    expect(parseOmniQuery('arch', 'notes')).toEqual({ kind: 'NOTE', query: 'arch' });
  });
  it('defaults to commands mode', () => {
    expect(parseOmniQuery('x')).toEqual({ kind: 'CMD', query: 'x' });
  });
});

describe('CommandPalette — open / closed', () => {
  it('renders nothing visible when open is false', () => {
    render(() => (
      <CommandPalette open={false} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(screen.queryByPlaceholderText(CMD_PLACEHOLDER)).not.toBeInTheDocument();
  });

  it('renders the palette input when open is true', () => {
    render(() => (
      <CommandPalette open={true} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(getInput()).toBeInTheDocument();
  });
});

describe('CommandPalette — single-purpose modes', () => {
  it('commands mode shows actions only — no notes, no GO/session rows', async () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'a', label: 'Run Tests' })]}
        onOpenChange={() => {}}
      />
    ));
    expect(screen.getByText('Run Tests')).toBeInTheDocument();
    expect(screen.getAllByText('CMD').length).toBeGreaterThan(0);
    // The old omnibox mixed in notes and surfaces; the palette must not.
    await Promise.resolve();
    expect(screen.queryByText('Architecture')).not.toBeInTheDocument();
    expect(screen.queryByText(/Session mode/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Edit mode/)).not.toBeInTheDocument();
  });

  it('notes mode lists kiln notes only, most recently updated first', async () => {
    render(() => (
      <CommandPalette open={true} mode="notes" commands={[cmd({ label: 'Hidden Cmd' })]} onOpenChange={() => {}} />
    ));
    await waitFor(() => {
      expect(screen.getByText('Architecture')).toBeInTheDocument();
    });
    expect(screen.queryByText('Hidden Cmd')).not.toBeInTheDocument();
    // Recency order: Roadmap (07-12) above Architecture (07-10).
    const labels = screen.getAllByText(/Roadmap|Architecture/).map((el) => el.textContent);
    expect(labels[0]).toBe('Roadmap');
    // Note paths render as descriptions.
    expect(screen.getByText('/kiln/Meta/Roadmap.md')).toBeInTheDocument();
  });

  it('mode sets the placeholder', () => {
    render(() => (
      <CommandPalette open={true} mode="notes" commands={[]} onOpenChange={() => {}} />
    ));
    expect(screen.getByPlaceholderText(NOTE_PLACEHOLDER)).toBeInTheDocument();
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

  it('fuzzy: notes match by path segments too', async () => {
    render(() => (
      <CommandPalette open={true} mode="notes" commands={[]} onOpenChange={() => {}} />
    ));
    await waitFor(() => {
      expect(screen.getByText('Roadmap')).toBeInTheDocument();
    });
    fireEvent.input(getInput(NOTE_PLACEHOLDER), { target: { value: 'meta' } });
    // "meta" hits Roadmap's path segment and Architecture's tag.
    expect(screen.getByText('Roadmap')).toBeInTheDocument();
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

  it('[[ crosses over from commands mode to note search', async () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(), { target: { value: '[[arch' } });
    await waitFor(() => {
      expect(screen.getByText('Architecture')).toBeInTheDocument();
    });
    expect(screen.queryByText('Compile Project')).not.toBeInTheDocument();
  });

  it('> crosses over from notes mode to commands', async () => {
    render(() => (
      <CommandPalette open={true} mode="notes" commands={commands} onOpenChange={() => {}} />
    ));
    fireEvent.input(getInput(NOTE_PLACEHOLDER), { target: { value: '>compile' } });
    expect(screen.getByText('Compile Project')).toBeInTheDocument();
    expect(screen.queryByText('Architecture')).not.toBeInTheDocument();
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

  it('shows the prefix hint footer with both bindings', () => {
    render(() => (
      <CommandPalette open={true} commands={[]} onOpenChange={() => {}} />
    ));
    expect(screen.getByText('command')).toBeInTheDocument();
    expect(screen.getByText('note')).toBeInTheDocument();
    expect(screen.getByText(/Ctrl\+P commands · Ctrl\+O notes/)).toBeInTheDocument();
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
});
