import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ToolCard } from '../ToolCard';
import type { ToolCallDisplay } from '@/lib/types';

function makeTool(overrides: Partial<ToolCallDisplay> = {}): ToolCallDisplay {
  return {
    id: 'tc-1',
    name: 'submit_answer',
    args: '{}',
    status: 'complete',
    result: 'final',
    ...overrides,
  };
}

describe('ToolCard — terminate badge', () => {
  it('renders the badge when terminate is true', () => {
    render(() => <ToolCard toolCall={makeTool({ terminate: true })} />);

    const badge = screen.getByText('Terminated');
    expect(badge).toBeInTheDocument();
    expect(badge.getAttribute('title')).toBe('This tool ended the agent turn early.');
  });

  it('does not render the badge when terminate is false', () => {
    render(() => <ToolCard toolCall={makeTool({ terminate: false })} />);
    expect(screen.queryByText('Terminated')).not.toBeInTheDocument();
  });

  it('does not render the badge when terminate is undefined (legacy events)', () => {
    render(() => <ToolCard toolCall={makeTool()} />);
    expect(screen.queryByText('Terminated')).not.toBeInTheDocument();
  });
});
