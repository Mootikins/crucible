import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { AskInteraction } from '../interactions/AskInteraction';
import { PopupInteraction } from '../interactions/PopupInteraction';
import { PermissionInteraction } from '../interactions/PermissionInteraction';
import { InteractionHandler } from '../interactions/InteractionHandler';
import type { InteractionOf, InteractionRequest } from '@/lib/types';

// `@/lib/api` is NOT mocked. The permission prompt reads the file on disk
// through the shared cache of `lib/query/fs.ts`, and the mocked `fetch` below
// answers the daemon's route — so nothing here reaches the network, and the
// read the prompt makes is one a test can see.
let env: TestQueryEnv;
/** The path of every file read the daemon answered, in order. */
let reads: string[] = [];

beforeEach(() => {
  reads = [];
  env = createTestQueryEnv({
    'GET /api/kiln/file': (request: Request) => {
      reads.push(new URL(request.url).searchParams.get('path') ?? '');
      return { content: 'on disk\n', content_hash: 'hash-1' };
    },
  });
});

afterEach(() => {
  env.restore();
});

// Mock DiffViewer used by PermissionInteraction
vi.mock('@/components/DiffViewer', () => ({
  DiffViewer: () => <div data-testid="diff-viewer" />,
}));

// ---------------------------------------------------------------------------
// AskInteraction
// ---------------------------------------------------------------------------

describe('AskInteraction', () => {
  const mockOnRespond = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the question text', () => {
    const request: InteractionOf<'ask'> = {
      kind: 'ask',
      id: 'ask-1',
      question: 'Which framework do you prefer?',
      choices: ['SolidJS', 'React'],
    };

    render(() => <AskInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Which framework do you prefer?')).toBeInTheDocument();
  });

  it('renders choices as selectable options', () => {
    const request: InteractionOf<'ask'> = {
      kind: 'ask',
      id: 'ask-2',
      question: 'Pick one',
      choices: ['Alpha', 'Beta', 'Gamma'],
    };

    render(() => <AskInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Alpha')).toBeInTheDocument();
    expect(screen.getByText('Beta')).toBeInTheDocument();
    expect(screen.getByText('Gamma')).toBeInTheDocument();
  });

  it('calls onRespond with selected index on submit', async () => {
    const request: InteractionOf<'ask'> = {
      kind: 'ask',
      id: 'ask-3',
      question: 'Pick a color',
      choices: ['Red', 'Blue'],
    };

    render(() => <AskInteraction request={request} onRespond={mockOnRespond} />);

    // Select the second choice (Blue, index 1)
    const radios = screen.getAllByRole('radio');
    await fireEvent.click(radios[1]);

    // Submit
    const submitButton = screen.getByText('Submit');
    await fireEvent.click(submitButton);

    expect(mockOnRespond).toHaveBeenCalledWith({
      kind: 'ask',
      selected: [1],
      other: undefined,
    });
  });
});

// ---------------------------------------------------------------------------
// PopupInteraction
// ---------------------------------------------------------------------------

describe('PopupInteraction', () => {
  const mockOnRespond = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the title and entry labels', () => {
    const request: InteractionOf<'popup'> = {
      kind: 'popup',
      id: 'popup-1',
      title: 'Select a file',
      entries: [
        { label: 'README.md', description: 'Project readme' },
        { label: 'AGENTS.md' },
      ],
    };

    render(() => <PopupInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Select a file')).toBeInTheDocument();
    expect(screen.getByText('README.md')).toBeInTheDocument();
    expect(screen.getByText('AGENTS.md')).toBeInTheDocument();
  });

  it('renders entry descriptions when present', () => {
    const request: InteractionOf<'popup'> = {
      kind: 'popup',
      id: 'popup-2',
      title: 'Choose',
      entries: [
        { label: 'Option A', description: 'First option details' },
        { label: 'Option B' },
      ],
    };

    render(() => <PopupInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('First option details')).toBeInTheDocument();
  });

  it('calls onRespond with selected_index when entry clicked', async () => {
    const request: InteractionOf<'popup'> = {
      kind: 'popup',
      id: 'popup-3',
      title: 'Pick one',
      entries: [
        { label: 'First' },
        { label: 'Second' },
        { label: 'Third' },
      ],
    };

    render(() => <PopupInteraction request={request} onRespond={mockOnRespond} />);

    // Click the second entry
    await fireEvent.click(screen.getByText('Second'));

    expect(mockOnRespond).toHaveBeenCalledWith({ kind: 'popup', selected_index: 1 });
  });
});

// Responses carry an explicit `kind`. The server can infer a tag for the three
// bare shapes older clients sent (`tag_interaction_response`), but inference
// cannot separate a panel result from an ask response — both carry `selected` —
// so every component states its kind rather than relying on the guess.
// ---------------------------------------------------------------------------
// PermissionInteraction
// ---------------------------------------------------------------------------

describe('PermissionInteraction', () => {
  const mockOnRespond = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders Allow and Deny buttons', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-1',
      action: { type: 'bash', tokens: ['ls', '-la'] },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Allow')).toBeInTheDocument();
    expect(screen.getByText('Deny')).toBeInTheDocument();
  });

  // Two adjacent buttons of equal weight told the user the choices cost the
  // same. Allowing lets an agent run a command or write to disk; denying
  // costs a retry.
  it('weights the consent pair: Deny first and quiet, Allow marked as the consequential one', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-weight',
      action: { type: 'bash', tokens: ['rm', '-rf', 'build'] },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    const deny = screen.getByTestId('perm-deny');
    const allow = screen.getByTestId('perm-allow');

    // Deny is the first tab stop and the first thing read.
    expect(deny.compareDocumentPosition(allow) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Deny is the quiet neutral, NOT the red that framed the safe choice as
    // the dangerous one.
    expect(deny.className).toContain('bg-surface-elevated');
    expect(deny.className).not.toContain('error');
    // Allow wears `attention` — a decision — not `primary`'s friendly confirm.
    expect(allow.className).toContain('attention');
    expect(allow.className).not.toContain('primary');
    // ...and the two do not share a treatment.
    expect(allow.className).not.toBe(deny.className);
  });

  it('names a tool exactly once, in the action chip', () => {
    // The card used to carry a generic "Tool" chip plus a "Tool: <name>"
    // line: the word twice, two lines for one fact.
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-tool-dup',
      action: { type: 'tool', name: 'write_file', args: { path: 'notes/a.md' } },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByTestId('perm-action-chip')).toHaveTextContent('write_file');
    expect(screen.queryByText(/^Tool:/)).not.toBeInTheDocument();
    expect(screen.queryByText('Tool', { exact: true })).not.toBeInTheDocument();
  });

  it('shows the agent, the wire name and the layer that asked', () => {
    // An ACP command asks as a bash request; the call keeps the raw name.
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-about',
      action: { type: 'bash', tokens: ['cargo test'] },
      call: {
        kind: 'command',
        tool: 'command',
        command: 'cargo test',
        agent: 'codex',
        raw: { name: 'exec_command' },
        render: { line: 'cargo test' },
      },
      layer: 'ask mode',
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByTestId('perm-about')).toHaveTextContent(
      'agent codex · wire name exec_command · asked by ask mode',
    );
  });

  it('draws the render line and the render fields in place of the arguments', () => {
    // The daemon rendered the call; the card draws the table and does not
    // read the arguments. A plugin render changes what the prompt shows.
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-render',
      action: { type: 'tool', name: 'spawn', args: { prompt: 'from the arguments' } },
      call: {
        kind: 'delegate',
        tool: 'spawn',
        render: { line: 'fix the parser', fields: [{ label: 'agent', value: 'claude' }] },
      },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('fix the parser')).toBeInTheDocument();
    expect(screen.getByTestId('perm-tool-args').textContent).toBe('agent=claude');
    expect(screen.queryByText(/from the arguments/)).not.toBeInTheDocument();
  });

  // Decision 6: a prompt in a plugin turn names the plugin whose turn asks.
  it('names the plugin whose turn asks', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission', id: 'perm-plugin', action: { type: 'bash', tokens: ['ls'] },
      origin: { kind: 'plugin', name: 'goal' },
    };
    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);
    expect(screen.getByText('goal requests permission')).toBeInTheDocument();
  });

  it('keeps the verb chip for non-tool permissions', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-bash-chip',
      action: { type: 'bash', tokens: ['rm', '-rf', 'build'] },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    // "Execute" identifies a bash request; there is no tool name to show.
    expect(screen.getByTestId('perm-action-chip')).toHaveTextContent('Execute');
  });

  it('shows every tool argument in full for tool permissions', () => {
    const longQuery = 'find all callers of parse_provider_model across the workspace ' + 'x'.repeat(80);
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-tool-1',
      action: {
        type: 'tool',
        name: 'search_vectors',
        args: {
          query: longQuery,
          limit: 20,
          filters: { kiln: 'docs', tags: ['api'] },
        },
      },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    const args = screen.getByTestId('perm-tool-args');
    // Every key visible, string values verbatim (no truncation), non-strings
    // pretty-printed JSON (multi-line for readability — see prettyPrintMaybeJson
    // in PermissionInteraction).
    expect(args.textContent).toContain(`query=${longQuery}`);
    expect(args.textContent).toContain('limit=20');
    expect(args.textContent).toContain('"kiln": "docs"');
    expect(args.textContent).toContain('"tags": [');
    expect(args.textContent).toContain('"api"');
    // The empty-tokens fallback box must not add a misleading "(no arguments)".
    expect(screen.queryByText('(no arguments)')).not.toBeInTheDocument();
  });

  // The prompt shows what the write would REPLACE, so it reads the file on
  // disk. It reads it through the same cache entry the editor and the tool
  // card hold, so a prompt about an open file costs no second read.
  it('renders the diff the daemon attached to the write, reading no file', async () => {
    // The request carries the change's authoritative form (daemon FileDiffs,
    // forwarded whole by the web server): the prompt no longer guesses a path
    // out of tokens or fetches a baseline itself.
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-write-1',
      action: { type: 'write', segments: ['/kiln/notes/a.md'] },
      diffs: [{ path: '/kiln/notes/a.md', old_content: 'on disk\n', new_content: 'typed\n' }],
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await waitFor(() => expect(screen.getByTestId('diff-viewer')).toBeInTheDocument());
    expect(reads).toEqual([]);
  });

  it('keeps the (no arguments) fallback for tool permissions without args', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-tool-2',
      action: { type: 'tool', name: 'list_notes', args: {} },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);
    expect(screen.getByText('(no arguments)')).toBeInTheDocument();
  });

  it('renders the action type label', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-2',
      action: { type: 'bash', tokens: ['echo', 'hello'] },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Execute')).toBeInTheDocument();
    expect(screen.getByText('Permission Required')).toBeInTheDocument();
  });

  it('calls onRespond with allowed=true when Allow clicked', async () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-3',
      action: { type: 'bash', tokens: ['rm', '-rf', '/tmp/test'] },
      pattern: 'rm -rf /tmp/test',
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await fireEvent.click(screen.getByText('Allow'));

    expect(mockOnRespond).toHaveBeenCalledWith({
      kind: 'permission',
      allowed: true,
      pattern: 'rm -rf /tmp/test',
      scope: 'once',
    });
  });

  // The daemon checks a grant against the canonical call. A Claude `Edit` is
  // a file edit, so its grant is the path, not the tool name on the wire.
  it('answers with the session grant directly from the shortcut', () => {
    render(() => <PermissionInteraction request={{ kind: 'permission', id: 'session-scope', action: { type: 'tool', name: 'Edit', args: {} }, pattern: '/w/a.rs' }} onRespond={mockOnRespond} />);
    fireEvent.click(screen.getByRole('button', { name: /^Allow for session$/ }));
    expect(mockOnRespond).toHaveBeenCalledWith({ kind: 'permission', allowed: true, pattern: '/w/a.rs', scope: 'session' });
  });

  it('sends the grant that the daemon suggested, not a token of the request', async () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-5',
      action: { type: 'tool', name: 'Edit', args: {} },
      pattern: '/w/a.rs',
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await fireEvent.click(screen.getByTestId('perm-scopes-toggle'));
    await fireEvent.click(screen.getByText('Project'));
    await fireEvent.click(screen.getByText('Allow'));

    expect(mockOnRespond).toHaveBeenCalledWith({
      kind: 'permission',
      allowed: true,
      pattern: '/w/a.rs',
      scope: 'project',
    });
  });

  // With no grant that can name the call, a wider scope would save nothing.
  it('offers the scopes only when the daemon suggested a grant', () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-6',
      action: { type: 'tool', name: 'tool', args: {} },
    };
    const { unmount } = render(() => (
      <PermissionInteraction request={request} onRespond={mockOnRespond} />
    ));
    expect(screen.queryByTestId('perm-scopes-toggle')).not.toBeInTheDocument();
    unmount();

    render(() => (
      <PermissionInteraction request={{ ...request, pattern: 'mcp__github__create_pr' }} onRespond={mockOnRespond} />
    ));
    expect(screen.getByTestId('perm-scopes-toggle')).toBeInTheDocument();
  });

  it('calls onRespond with allowed=false when Deny clicked', async () => {
    const request: InteractionOf<'permission'> = {
      kind: 'permission',
      id: 'perm-4',
      action: { type: 'tool', name: 'exec_sql', args: {} },
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await fireEvent.click(screen.getByText('Deny'));

    expect(mockOnRespond).toHaveBeenCalledWith({
      kind: 'permission',
      allowed: false,
      scope: 'once',
    });
  });
});

// ---------------------------------------------------------------------------
// InteractionHandler (dispatch)
// ---------------------------------------------------------------------------

describe('InteractionHandler', () => {
  const mockOnRespond = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders AskInteraction for ask kind', () => {
    const request: InteractionRequest = {
      kind: 'ask',
      id: 'ask-dispatch',
      question: 'Dispatched question?',
      choices: ['Yes', 'No'],
    };

    render(() => <InteractionHandler request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Dispatched question?')).toBeInTheDocument();
    expect(screen.getByText('Yes')).toBeInTheDocument();
    expect(screen.getByText('No')).toBeInTheDocument();
  });

  it('renders PermissionInteraction for permission kind', () => {
    const request: InteractionRequest = {
      kind: 'permission',
      id: 'perm-dispatch',
      action: { type: 'read', segments: ['/etc/passwd'] },
    };

    render(() => <InteractionHandler request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Allow')).toBeInTheDocument();
    expect(screen.getByText('Deny')).toBeInTheDocument();
    expect(screen.getByText('Read')).toBeInTheDocument();
  });
});
