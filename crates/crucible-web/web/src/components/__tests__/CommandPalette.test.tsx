import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { CommandPalette, type PaletteCommand } from '../CommandPalette';

// Kobalte's Dialog renders into a Portal appended to document.body. Even
// though solid-testing-library auto-cleans the render container, the
// portaled dialog content can persist across tests (sentinel focus-trap
// nodes, leftover Kobalte presence wrappers). Wipe the body before each
// test so `screen.*` queries see only the current test's portal.
beforeEach(() => {
  document.body.innerHTML = '';
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

describe('CommandPalette — open / closed', () => {
  it('renders nothing visible when open is false', () => {
    render(() => (
      <CommandPalette open={false} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(screen.queryByPlaceholderText('Type a command or search...')).not.toBeInTheDocument();
  });

  it('renders the search input when open is true', () => {
    render(() => (
      <CommandPalette open={true} commands={[cmd()]} onOpenChange={() => {}} />
    ));
    expect(screen.getByPlaceholderText('Type a command or search...')).toBeInTheDocument();
  });
});

describe('CommandPalette — grouping', () => {
  it('renders each command label exactly once', () => {
    const commands = [
      cmd({ id: 'a', label: 'Run Tests', category: 'Chat' }),
      cmd({ id: 'b', label: 'New Session', category: 'Session' }),
      cmd({ id: 'c', label: 'Go Home', category: 'Navigation' }),
      cmd({ id: 'd', label: 'Open Settings', category: 'Settings' }),
    ];
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    expect(screen.getByText('Run Tests')).toBeInTheDocument();
    expect(screen.getByText('New Session')).toBeInTheDocument();
    expect(screen.getByText('Go Home')).toBeInTheDocument();
    expect(screen.getByText('Open Settings')).toBeInTheDocument();
  });

  it('renders headings in declared CATEGORY_ORDER (Chat → Settings)', () => {
    const commands = [
      // Intentionally provided in reverse order — the palette must reorder.
      cmd({ id: 's', label: 'Settings One', category: 'Settings' }),
      cmd({ id: 'n', label: 'Nav One', category: 'Navigation' }),
      cmd({ id: 'sess', label: 'Sess One', category: 'Session' }),
      cmd({ id: 'c', label: 'Chat One', category: 'Chat' }),
    ];
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    // Kobalte portals into document.body; query there. Also cmdk renders
    // its own headings (without the [cmdk-group-heading=""] attr we put on
    // the inner div) — restrict to the divs we explicitly tagged.
    const headings = Array.from(document.body.querySelectorAll('div[cmdk-group-heading=""].tracking-wide'))
      .map((el) => el.textContent?.trim());
    expect(headings).toEqual(['Chat', 'Session', 'Navigation', 'Settings']);
  });

  it('omits empty category groups', () => {
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'a', label: 'Only Chat', category: 'Chat' })]}
        onOpenChange={() => {}}
      />
    ));
    const headings = Array.from(document.body.querySelectorAll('div[cmdk-group-heading=""].tracking-wide'))
      .map((el) => el.textContent?.trim());
    expect(headings).toEqual(['Chat']);
  });
});

describe('CommandPalette — filtering', () => {
  const commands = [
    cmd({ id: 'a', label: 'Compile Project', category: 'Chat', keywords: ['build', 'make'] }),
    cmd({ id: 'b', label: 'Switch Workspace', category: 'Session' }),
    cmd({ id: 'c', label: 'Open Files', category: 'Navigation' }),
  ];

  it('filters by case-insensitive label substring', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'workspace' } });

    expect(screen.queryByText('Compile Project')).not.toBeInTheDocument();
    expect(screen.getByText('Switch Workspace')).toBeInTheDocument();
    expect(screen.queryByText('Open Files')).not.toBeInTheDocument();
  });

  it('filters by keyword', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'build' } });

    // "Compile Project" matches via keyword
    expect(screen.getByText('Compile Project')).toBeInTheDocument();
    expect(screen.queryByText('Switch Workspace')).not.toBeInTheDocument();
  });

  it('description text alone does NOT match — only label and keywords reach cmdk', () => {
    // The component's matchesQuery helper considers descriptions, but cmdk's
    // own filter only sees value (= label) and keywords. So a query that
    // matches only the description filters everything out. This test pins
    // current behavior; if we ever route description through keywords, flip
    // the assertion.
    render(() => (
      <CommandPalette
        open={true}
        commands={[
          cmd({ id: 'x', label: 'Toggle Mode', category: 'Chat', description: 'flip between plan and normal' }),
        ]}
        onOpenChange={() => {}}
      />
    ));
    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'plan' } });

    expect(screen.queryByText('Toggle Mode')).not.toBeInTheDocument();
  });

  it('shows "No commands match" when the query matches nothing', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'zzzznomatch' } });

    expect(screen.getByText(/No commands match/)).toBeInTheDocument();
  });

  it('treats whitespace-only query as empty (shows everything)', () => {
    render(() => (
      <CommandPalette open={true} commands={commands} onOpenChange={() => {}} />
    ));
    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '   ' } });

    expect(screen.getByText('Compile Project')).toBeInTheDocument();
    expect(screen.getByText('Switch Workspace')).toBeInTheDocument();
    expect(screen.getByText('Open Files')).toBeInTheDocument();
  });
});

describe('CommandPalette — selection', () => {
  it('invokes the action and closes the palette when an item is selected', () => {
    const action = vi.fn();
    const onOpenChange = vi.fn();
    render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ id: 'a', label: 'Trigger Me', category: 'Chat', action })]}
        onOpenChange={onOpenChange}
      />
    ));
    // Find the rendered item label and click its enclosing item element.
    const labelEl = screen.getByText('Trigger Me');
    // Walk up to the cmdk Command.Item element (carries data-selected attr).
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
        commands={[cmd({ label: 'Save', category: 'Chat', shortcut: '⌘S' })]}
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
        commands={[cmd({ label: 'Compact', category: 'Chat', description: 'Squash conversation history' })]}
        onOpenChange={() => {}}
      />
    ));
    expect(screen.getByText('Squash conversation history')).toBeInTheDocument();
  });

  it('omits the kbd element when no shortcut is set', () => {
    const { container } = render(() => (
      <CommandPalette
        open={true}
        commands={[cmd({ label: 'NoShortcut', category: 'Chat' })]}
        onOpenChange={() => {}}
      />
    ));
    expect(container.querySelector('kbd')).toBeNull();
  });
});

describe('CommandPalette — query reset on close', () => {
  it('clears the query when the palette is closed', () => {
    const [open, setOpen] = createSignal(true);
    render(() => (
      <CommandPalette
        open={open()}
        commands={[cmd({ id: 'a', label: 'AAA', category: 'Chat' })]}
        onOpenChange={setOpen}
      />
    ));

    const input = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'foo' } });
    expect(input.value).toBe('foo');

    setOpen(false);
    setOpen(true);

    const reopenedInput = screen.getByPlaceholderText('Type a command or search...') as HTMLInputElement;
    expect(reopenedInput.value).toBe('');
  });
});
