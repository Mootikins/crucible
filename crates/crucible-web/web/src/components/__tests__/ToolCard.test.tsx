import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { ToolCard } from '../ToolCard';
import type { ToolCallDisplay } from '@/lib/types';

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

describe('ToolCard — collapsed header', () => {
  it('starts collapsed by default and shows only the header row', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    expect(screen.getByText('read_file')).toBeInTheDocument();
    // Arguments section title is only rendered when expanded
    expect(screen.queryByText('Arguments')).not.toBeInTheDocument();
    expect(screen.queryByText('Result')).not.toBeInTheDocument();
  });

  it('shows the right-facing caret while collapsed and flips to down when expanded', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    expect(screen.getByText('▶')).toBeInTheDocument();
    fireEvent.click(screen.getByText('read_file'));
    expect(screen.getByText('▼')).toBeInTheDocument();
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
  // Header icon precedes the tool name. We assert via getByText since each
  // emoji is unique enough in the collapsed header to avoid ambiguity.
  const cases: Array<[string, string]> = [
    ['read_file', '📄'],
    ['file_lookup', '📄'],
    ['write_note', '✏️'],
    ['edit_block', '✏️'],
    ['search_codebase', '🔍'],
    ['find_refs', '🔍'],
    ['bash_exec', '⚡'],
    ['run_shell', '⚡'],
    ['exec_command', '⚡'],
    ['web_fetch', '🌐'],
    ['http_get', '🌐'],
    ['fetch_url', '🌐'],
    ['note_create', '📝'],
    ['memory_get', '📝'],
    ['weird_tool_name', '🔧'],
  ];

  for (const [name, emoji] of cases) {
    it(`maps "${name}" to ${emoji}`, () => {
      render(() => <ToolCard toolCall={makeTool({ name })} />);
      expect(screen.getByText(emoji)).toBeInTheDocument();
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
