import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { ToolCallDisplay } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// ===== Mock topology =====
// DiffViewer / MultiEditDiff are stubbed because their real implementations
// pull in Shiki tokenization (see ToolCard.integration.test.tsx for the real
// path, which CANNOT be merged here). The rich factories preserve the props
// the diff-rendering suite asserts on (data-file, data-count, old:|new: text);
// the open-in-editor suite only checks the button, so the rich stub satisfies
// both.
vi.mock('../DiffViewer', () => ({
  DiffViewer: (props: { fileName?: string; oldContent: string; newContent: string }) => (
    <div data-testid="diff-viewer" data-file={props.fileName}>
      old:{props.oldContent}|new:{props.newContent}
    </div>
  ),
}));
vi.mock('../MultiEditDiff', () => ({
  MultiEditDiff: (props: { fileName: string; edits: unknown[] }) => (
    <div data-testid="multi-edit-diff" data-file={props.fileName} data-count={props.edits.length} />
  ),
}));

// Open-in-editor mocks. Only the "Open in editor" describe block triggers
// these; the other suites render ToolCards without clicking the button, so
// these mocks are inert for them. vi.clearAllMocks runs between every test
// via the global `clearMocks: true` in vite.config.ts.
const CURRENT = 'top\nlet x = 1;\nbottom\n';
const openFileWithDiffMock = vi.fn();
const addNotificationMock = vi.fn();

// `@/lib/api` is NOT mocked. The card reads the file through the shared cache
// of `lib/query/fs.ts`, so the daemon's own route is what answers, and the
// repeat-click case below counts the reads that reached it.
let env: TestQueryEnv;
/** The path of every read the daemon answered, in order. */
let reads: string[] = [];
/** How the daemon answers a read. Replaced by the cases that need a delay or a 404. */
let answerRead: () => unknown = () => ({ content: CURRENT, content_hash: 'hash-1' });

/** A daemon that does not have the file. */
function missing(): Response {
  return new Response(JSON.stringify({ error: { code: 404, message: 'no such file' } }), {
    status: 404,
    headers: { 'Content-Type': 'application/json' },
  });
}

beforeEach(() => {
  reads = [];
  answerRead = () => ({ content: CURRENT, content_hash: 'hash-1' });
  env = createTestQueryEnv({
    'GET /api/kiln/file': (request: Request) => {
      reads.push(new URL(request.url).searchParams.get('path') ?? '');
      return answerRead();
    },
  });
});

afterEach(() => {
  env.restore();
});
vi.mock('@/lib/file-actions', () => ({
  openFileWithDiff: (...a: unknown[]) => openFileWithDiffMock(...a),
}));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotificationMock(...a) },
}));

import { ToolCard } from '../ToolCard';

// ===== Shared helpers =====
function makeTool(overrides: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    name: 'read_file',
    args: '{}',
    status: 'complete',
    result: 'ok',
    ...overrides,
  };
}

// Diff-rendering helper: a default Edit-shaped call that the diff tests
// override per-case.
function call(overrides: Partial<ToolCallDisplay>): ToolCallDisplay {
  return {
    id: 'tc-1',
    name: 'Edit',
    args: '',
    status: 'complete',
    ...overrides,
  };
}

function expandCard(container: HTMLElement) {
  // Only click if currently collapsed — error-status cards auto-expand.
  const button = container.querySelector('button');
  if (button && button.getAttribute('aria-expanded') !== 'true') {
    fireEvent.click(button);
  }
}

// Default tool for the terminate-badge suite. Uses submit_answer (no diff
// rendering) so the DiffViewer/MultiEditDiff mocks above are never reached.
function makeTerminateTool(overrides: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    name: 'submit_answer',
    args: '{}',
    status: 'complete',
    result: 'final',
    ...overrides,
  };
}

// Edit-tool fixture for the open-in-editor suite. The diff comes from the
// daemon's FileDiff projection — the card no longer derives one from args.
const editTool = (): ToolCallDisplay => ({
  id: 'tc-1',
  name: 'Edit',
  status: 'complete',
  args: JSON.stringify({ file_path: '/proj/app.ts' }),
  diffs: [{ path: '/proj/app.ts', old_content: 'let x = 1;', new_content: 'let x = 42;' }],
});

describe('ToolCard — collapsed header', () => {
  it('starts collapsed by default and shows only the header row', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    expect(screen.getByText('read_file')).toBeInTheDocument();
    // Arguments section title is only rendered when expanded
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
    expect(screen.queryByText('Result')).not.toBeInTheDocument();
  });

  it('exposes collapsed/expanded state via aria-expanded', () => {
    const { container } = render(() => <ToolCard toolCall={makeTool()} />);
    const button = container.querySelector('button')!;
    expect(button.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(screen.getByText('read_file'));
    expect(button.getAttribute('aria-expanded')).toBe('true');
  });

  it('toggles back to collapsed on a second click', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    const trigger = screen.getByText('read_file');
    fireEvent.click(trigger);
    expect(screen.getByText('Result')).toBeInTheDocument();
    fireEvent.click(trigger);
    expect(screen.queryByText('Result')).not.toBeInTheDocument();
  });
});

describe('ToolCard — icon selection', () => {
  // Header icon precedes the tool name; lucide stamps a kebab-case class on
  // the rendered svg, which is the stable hook for which icon was chosen.
  const cases: Array<[string, string]> = [
    ['read_file', 'lucide-file-text'],
    ['file_lookup', 'lucide-file-text'],
    ['write_note', 'lucide-pencil'],
    ['edit_block', 'lucide-pencil'],
    ['search_codebase', 'lucide-search'],
    ['find_refs', 'lucide-search'],
    ['bash_exec', 'lucide-zap'],
    ['run_shell', 'lucide-zap'],
    ['exec_command', 'lucide-zap'],
    ['web_fetch', 'lucide-globe'],
    ['http_get', 'lucide-globe'],
    ['fetch_url', 'lucide-globe'],
    ['note_create', 'lucide-sticky-note'],
    ['memory_get', 'lucide-sticky-note'],
    ['weird_tool_name', 'lucide-wrench'],
  ];

  for (const [name, iconClass] of cases) {
    it(`maps "${name}" to ${iconClass}`, () => {
      const { container } = render(() => <ToolCard toolCall={makeTool({ name })} />);
      expect(container.querySelector(`svg.${iconClass}`)).toBeInTheDocument();
    });
  }
});

describe('ToolCard — status indicators', () => {
  it('renders the running spinner via title attribute', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'running', result: undefined })} />);
    expect(screen.getByTitle('Running')).toBeInTheDocument();
  });

  it('renders a check on complete', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'complete' })} />);
    expect(screen.getByTitle('Complete')).toBeInTheDocument();
    expect(screen.getByText('✓')).toBeInTheDocument();
  });

  it('renders an X on error', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'error', result: 'boom' })} />);
    expect(screen.getByTitle('Error')).toBeInTheDocument();
    expect(screen.getByText('✗')).toBeInTheDocument();
  });
});

describe('ToolCard — auto-expand on error', () => {
  it('starts expanded when initial status is error', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'error', result: 'crash' })} />);
    // Error label appears in the result section heading
    expect(screen.getByText('Error')).toBeInTheDocument();
    expect(screen.getByText('crash')).toBeInTheDocument();
  });

  it('switches the result label to "Error" (not "Result") on error', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'error', result: 'msg' })} />);
    expect(screen.getByText('Error')).toBeInTheDocument();
    expect(screen.queryByText('Result')).not.toBeInTheDocument();
  });
});

describe('ToolCard — args formatting', () => {
  it('pretty-prints valid JSON args when expanded', () => {
    render(() => (
      <ToolCard toolCall={makeTool({ args: '{"a":1,"b":[2,3]}' })} />
    ));
    fireEvent.click(screen.getByText('read_file'));
    const pre = screen.getByText(/"a": 1/);
    expect(pre.textContent).toContain('"b": [');
    expect(pre.textContent).toContain('2');
    expect(pre.textContent).toContain('3');
  });

  it('falls back to raw text when args are not valid JSON', () => {
    render(() => <ToolCard toolCall={makeTool({ args: 'not-json' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('not-json')).toBeInTheDocument();
  });

  it('hides the Arguments section when args is empty string', () => {
    render(() => <ToolCard toolCall={makeTool({ args: '' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
  });

  it('hides the Arguments section when args is literal `""`', () => {
    render(() => <ToolCard toolCall={makeTool({ args: '""' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
  });

  it('still renders the Arguments heading for object args', () => {
    render(() => <ToolCard toolCall={makeTool({ args: '{"x":1}' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('Arguments')).toBeInTheDocument();
  });
});

describe('ToolCard — bash command rendering', () => {
  // The daemon's display projection is what makes a call render as a shell
  // line; a bare `command` argument no longer earns the Command block.
  const bash = (command: string, extra: Record<string, unknown> = {}) =>
    makeTool({
      name: 'bash',
      args: JSON.stringify({ command, ...extra }),
      display: { kind: 'command', primary: command },
    });

  it('renders a bash command as a shell line, not JSON', () => {
    render(() => <ToolCard toolCall={bash('ls -la /tmp')} />);
    fireEvent.click(screen.getByText('bash'));

    const cmd = screen.getByTestId('bash-command');
    expect(cmd.textContent).toContain('ls -la /tmp');
    // The JSON envelope is noise around the one thing that matters.
    expect(cmd.textContent).not.toContain('"command"');
    expect(cmd.textContent).not.toContain('{');
  });

  it('keeps real newlines in a multi-line command', () => {
    // As JSON this rendered as one line containing a literal \n escape.
    render(() => <ToolCard toolCall={bash('cd /tmp\ngrep -r foo .')} />);
    fireEvent.click(screen.getByText('bash'));

    const cmd = screen.getByTestId('bash-command');
    expect(cmd.textContent).toContain('cd /tmp\ngrep -r foo .');
    expect(cmd.textContent).not.toContain('\\n');
  });

  it('still shows any non-command bash arguments', () => {
    render(() => <ToolCard toolCall={bash('ls', { timeout: 30 })} />);
    fireEvent.click(screen.getByText('bash'));

    expect(screen.getByTestId('bash-command').textContent).toContain('ls');
    const args = screen.getByTestId('tool-args');
    expect(args.textContent).toContain('timeout');
    expect(args.textContent).toContain('30');
    // …but not a second copy of the command.
    expect(args.textContent).not.toContain('"command"');
  });

  it('falls back to the JSON args block when bash has no command string', () => {
    render(() => <ToolCard toolCall={makeTool({ name: 'bash', args: '{"script":"x"}' })} />);
    fireEvent.click(screen.getByText('bash'));

    expect(screen.queryByTestId('bash-command')).not.toBeInTheDocument();
    expect(screen.getByTestId('tool-args').textContent).toContain('script');
  });

  it('does not treat a non-shell tool with a command arg as bash', () => {
    render(() => <ToolCard toolCall={makeTool({ name: 'run_task', args: '{"command":"build"}' })} />);
    fireEvent.click(screen.getByText('run_task'));

    expect(screen.queryByTestId('bash-command')).not.toBeInTheDocument();
  });
});

describe('ToolCard — result rendering', () => {
  it('shows the Result heading when result is present and status is not error', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'complete', result: 'final output' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('Result')).toBeInTheDocument();
    expect(screen.getByText('final output')).toBeInTheDocument();
  });

  it('omits the Result section when result is missing', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'complete', result: undefined })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByText('Result')).not.toBeInTheDocument();
  });

  it('shows an "Executing…" indicator while running without a result', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'running', result: undefined })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('Executing…')).toBeInTheDocument();
  });

  it('does not show "Executing…" once a partial result has streamed in', () => {
    render(() => <ToolCard toolCall={makeTool({ status: 'running', result: 'partial' })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByText('Executing…')).not.toBeInTheDocument();
    expect(screen.getByText('partial')).toBeInTheDocument();
  });
});

describe('ToolCard — ID footer', () => {
  it('prefers callId over id when both are present', () => {
    render(() => (
      <ToolCard toolCall={makeTool({ id: 'inner', callId: 'outer-call' })} />
    ));
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('ID: outer-call')).toBeInTheDocument();
  });

  it('falls back to id when callId is missing', () => {
    render(() => <ToolCard toolCall={makeTool({ id: 'only-id', callId: undefined })} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('ID: only-id')).toBeInTheDocument();
  });
});

describe('ToolCard — diff rendering', () => {
  it('renders DiffViewer from the recorded diff of a completed call', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Edit',
          args: JSON.stringify({ file_path: 'src/a.rs', old_string: 'x', new_string: 'y' }),
          diffs: [{ path: 'src/a.rs', old_content: 'x', new_content: 'y' }],
          result: 'edited',
        })}
      />
    ));
    expandCard(container);
    const dv = screen.getByTestId('diff-viewer');
    expect(dv).toBeInTheDocument();
    expect(dv.getAttribute('data-file')).toBe('src/a.rs');
  });

  it('renders a whole-file write from a null old side', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Write',
          diffs: [{ path: 'src/new.ts', old_content: null, new_content: 'hello' }],
          result: 'wrote',
        })}
      />
    ));
    expandCard(container);
    const dv = screen.getByTestId('diff-viewer');
    expect(dv.textContent).toContain('old:|new:hello');
  });

  it('merges several edits to one file into MultiEditDiff', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'MultiEdit',
          diffs: [
            { path: 'src/a.rs', old_content: 'a', new_content: 'b' },
            { path: 'src/a.rs', old_content: 'c', new_content: 'd' },
          ],
          result: 'multi-edited',
        })}
      />
    ));
    expandCard(container);
    const med = screen.getByTestId('multi-edit-diff');
    expect(med.getAttribute('data-count')).toBe('2');
  });

  it('renders one viewer per file when a call touches several', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'some_acp_editor',
          diffs: [
            { path: 'src/a.rs', old_content: 'a', new_content: 'b' },
            { path: 'src/b.rs', old_content: null, new_content: 'new file' },
          ],
        })}
      />
    ));
    expandCard(container);
    const viewers = screen.getAllByTestId('diff-viewer');
    expect(viewers).toHaveLength(2);
    expect(viewers[0].getAttribute('data-file')).toBe('src/a.rs');
    expect(viewers[1].getAttribute('data-file')).toBe('src/b.rs');
  });

  it('renders the proposed diff while the tool is still running', () => {
    // The daemon attaches the diff when the call is announced, so the change
    // is visible while it executes — same as the TUI card, and the same
    // content the permission prompt showed before approval.
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Edit',
          status: 'running',
          diffs: [{ path: 'a', old_content: 'x', new_content: 'y' }],
        })}
      />
    ));
    expandCard(container);
    expect(screen.getByTestId('diff-viewer')).toBeInTheDocument();
  });

  it('falls back to plain <pre> result for a call without diffs', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Bash',
          args: JSON.stringify({ command: 'ls' }),
          result: 'foo\nbar',
        })}
      />
    ));
    expandCard(container);
    expect(screen.queryByTestId('diff-viewer')).toBeNull();
    expect(container.textContent).toContain('foo');
    expect(container.textContent).toContain('bar');
  });

  it('still renders error result <pre> when diff is also present for failed Edit', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Edit',
          status: 'error',
          diffs: [{ path: 'a', old_content: 'x', new_content: 'y' }],
          result: 'string not found',
        })}
      />
    ));
    expandCard(container);
    const diffEl = screen.getByTestId('diff-viewer');
    expect(diffEl).toBeInTheDocument();
    expect(container.textContent).toContain('string not found');

    // Error pre should appear BEFORE the diff in DOM order so users see the
    // failure reason before scrolling past the failed-attempt diff.
    const errorPre = screen.getByText('string not found');
    const position = errorPre.compareDocumentPosition(diffEl);
    // DOCUMENT_POSITION_FOLLOWING (4) means diffEl follows errorPre.
    expect(position & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('suppresses the Arguments JSON section when a diff is rendered', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Edit',
          args: JSON.stringify({ file_path: 'src/a.rs', old_string: 'x', new_string: 'y' }),
          diffs: [{ path: 'src/a.rs', old_content: 'x', new_content: 'y' }],
          result: 'edited',
        })}
      />
    ));
    expandCard(container);
    // Diff IS in the DOM
    expect(screen.getByTestId('diff-viewer')).toBeInTheDocument();
    // Arguments heading and raw JSON keys are NOT
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
    expect(container.textContent).not.toContain('"old_string"');
    expect(container.textContent).not.toContain('"new_string"');
  });

  it('still shows the input for non-diff tools (e.g. Bash)', () => {
    // A non-diff tool's input must be visible. Its Command block comes from
    // the daemon's display projection; without one, the raw args render.
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Bash',
          args: JSON.stringify({ command: 'ls' }),
          result: 'a\nb',
        })}
      />
    ));
    expandCard(container);
    expect(screen.queryByTestId('diff-viewer')).toBeNull();
    expect(screen.getByTestId('tool-args').textContent).toContain('ls');
  });

  it('falls back to plain <pre> when args JSON is malformed', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={call({
          name: 'Edit',
          args: '{not valid json',
          result: 'result text',
        })}
      />
    ));
    expandCard(container);
    expect(screen.queryByTestId('diff-viewer')).toBeNull();
    expect(container.textContent).toContain('result text');
  });
});

describe('ToolCard — terminate badge', () => {
  it('renders the badge when terminate is true', () => {
    render(() => <ToolCard toolCall={makeTerminateTool({ terminate: true })} />);

    const badge = screen.getByText('Terminated');
    expect(badge).toBeInTheDocument();
    expect(badge.getAttribute('title')).toBe('This tool ended the agent turn early.');
  });

  it('does not render the badge when terminate is false', () => {
    render(() => <ToolCard toolCall={makeTerminateTool({ terminate: false })} />);
    expect(screen.queryByText('Terminated')).not.toBeInTheDocument();
  });

  it('does not render the badge when terminate is undefined (legacy events)', () => {
    render(() => <ToolCard toolCall={makeTerminateTool()} />);
    expect(screen.queryByText('Terminated')).not.toBeInTheDocument();
  });
});

describe('ToolCard — Open in editor', () => {
  it('opens the file with the current content diffed against the applied edit', async () => {
    render(() => <ToolCard toolCall={editTool()} />);
    // Expand the card to reveal the diff section + the button.
    fireEvent.click(screen.getByText('Edit'));

    const btn = await screen.findByTestId('tool-open-in-editor');
    fireEvent.click(btn);

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(reads).toEqual(['/proj/app.ts']);
    // original = current file; proposed = current with the edit applied.
    expect(openFileWithDiffMock).toHaveBeenCalledWith(
      '/proj/app.ts',
      CURRENT,
      'top\nlet x = 42;\nbottom\n',
      'app.ts',
    );
  });

  it('has no Open-in-editor button for a non-diff tool', () => {
    render(() => <ToolCard toolCall={{ id: 't', name: 'read_file', args: '{}', status: 'complete', result: 'ok' }} />);
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.queryByTestId('tool-open-in-editor')).not.toBeInTheDocument();
  });

  it('offers Open-in-editor while the tool is still running (the diff is already known)', () => {
    render(() => <ToolCard toolCall={{ ...editTool(), status: 'running' }} />);
    fireEvent.click(screen.getByText('Edit'));
    expect(screen.getByTestId('tool-open-in-editor')).toBeInTheDocument();
  });

  // An unreadable path with an Edit means we have no baseline; applying the
  // edit to '' would yield an EMPTY proposed document — a delete-everything
  // diff the user could save over their file.
  it('refuses to open an Edit diff when the file cannot be read', async () => {
    answerRead = () => missing();
    render(() => <ToolCard toolCall={editTool()} />);
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.click(await screen.findByTestId('tool-open-in-editor'));

    await waitFor(() => expect(addNotificationMock).toHaveBeenCalledTimes(1));
    expect(addNotificationMock.mock.calls[0][0]).toBe('warning');
    expect(openFileWithDiffMock).not.toHaveBeenCalled();
  });

  // A Write carries the whole file, so an unreadable path is just a new file.
  it('opens a Write diff against empty when the file does not exist yet', async () => {
    answerRead = () => missing();
    const writeTool: ToolCallDisplay = {
      id: 'tc-2',
      name: 'Write',
      args: '',
      status: 'complete',
      diffs: [{ path: '/proj/new.ts', old_content: null, new_content: 'fresh\n' }],
    };
    render(() => <ToolCard toolCall={writeTool} />);
    fireEvent.click(screen.getByText('Write'));
    fireEvent.click(await screen.findByTestId('tool-open-in-editor'));

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(openFileWithDiffMock).toHaveBeenCalledWith('/proj/new.ts', '', 'fresh\n', 'new.ts');
    expect(addNotificationMock).not.toHaveBeenCalled();
  });

  it('ignores repeat clicks while a diff is still being fetched', async () => {
    let release!: () => void;
    answerRead = () =>
      new Promise((resolve) => {
        release = () => resolve({ content: CURRENT, content_hash: 'hash-1' });
      });
    render(() => <ToolCard toolCall={editTool()} />);
    fireEvent.click(screen.getByText('Edit'));

    const btn = await screen.findByTestId('tool-open-in-editor');
    fireEvent.click(btn);
    fireEvent.click(btn);
    release();

    await waitFor(() => expect(openFileWithDiffMock).toHaveBeenCalledTimes(1));
    expect(reads).toHaveLength(1);
  });
});

describe('ToolCard — daemon-provided display projection', () => {
  it('prefers the daemon projection over its own heuristic', () => {
    // The daemon knows things a key-priority list cannot — a plugin's Lua
    // display hint, for one. Its answer must win.
    render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'run_task',
          args: JSON.stringify({ command: 'build', path: '/repo' }),
          display: { kind: 'command', primary: 'make build' },
        })}
      />
    ));
    fireEvent.click(screen.getByText('run_task'));
    expect(screen.getByTestId('bash-command').textContent).toContain('make build');
  });

  it('uses the projection for the collapsed one-line summary too', () => {
    // Header summary and Command block now read the same field, so they
    // cannot disagree about which argument matters.
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'semantic_search',
          args: JSON.stringify({ query: 'wikilinks' }),
          display: { kind: 'query', primary: 'wikilinks' },
        })}
      />
    ));
    expect(container.textContent).toContain('wikilinks');
  });

  it('shows only the first line of a multi-line command in the summary', () => {
    const { container } = render(() => (
      <ToolCard
        toolCall={makeTool({
          name: 'bash',
          args: JSON.stringify({ command: 'cd /tmp\ngrep -r foo .' }),
          display: { kind: 'command', primary: 'cd /tmp\ngrep -r foo .' },
        })}
      />
    ));
    // Collapsed: the header row must not carry the second line.
    const header = container.querySelector('button');
    expect(header?.textContent).toContain('cd /tmp');
    expect(header?.textContent).not.toContain('grep -r foo');
  });
});

describe('ToolCard — auto-approval marker', () => {
  it('marks a call whose permission was granted without asking', () => {
    // Without this, an auto-approved call is visually identical to one that
    // never needed permission — no record of what was granted on your behalf.
    render(() => <ToolCard toolCall={makeTool({ name: 'bash', autoApproved: 'auto mode' })} />);
    const badge = screen.getByTestId('tool-auto-approved');
    expect(badge).toBeInTheDocument();
    expect(badge.getAttribute('title')).toContain('auto mode');
  });

  it('shows no marker for an ordinary call', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    expect(screen.queryByTestId('tool-auto-approved')).not.toBeInTheDocument();
  });
});
