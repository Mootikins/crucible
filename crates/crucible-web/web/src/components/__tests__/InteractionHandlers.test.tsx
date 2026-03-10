import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import { AskInteraction } from '../interactions/AskInteraction';
import { PopupInteraction } from '../interactions/PopupInteraction';
import { PermissionInteraction } from '../interactions/PermissionInteraction';
import { InteractionHandler } from '../interactions/InteractionHandler';
import type {
  AskRequest,
  PopupRequest,
  PermRequest,
  InteractionRequest,
} from '@/lib/types';

// Mock the API module to prevent real network calls
vi.mock('@/lib/api', () => ({
  respondToInteraction: vi.fn(),
  getFileContent: vi.fn().mockResolvedValue(''),
}));

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
    const request: AskRequest = {
      kind: 'ask',
      id: 'ask-1',
      question: 'Which framework do you prefer?',
      choices: ['SolidJS', 'React'],
    };

    render(() => <AskInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Which framework do you prefer?')).toBeInTheDocument();
  });

  it('renders choices as selectable options', () => {
    const request: AskRequest = {
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
    const request: AskRequest = {
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
    const request: PopupRequest = {
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
    const request: PopupRequest = {
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
    const request: PopupRequest = {
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

    expect(mockOnRespond).toHaveBeenCalledWith({ selected_index: 1 });
  });
});

// ---------------------------------------------------------------------------
// PermissionInteraction
// ---------------------------------------------------------------------------

describe('PermissionInteraction', () => {
  const mockOnRespond = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders Allow and Deny buttons', () => {
    const request: PermRequest = {
      kind: 'permission',
      id: 'perm-1',
      action_type: 'bash',
      tokens: ['ls', '-la'],
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Allow')).toBeInTheDocument();
    expect(screen.getByText('Deny')).toBeInTheDocument();
  });

  it('renders the action type label', () => {
    const request: PermRequest = {
      kind: 'permission',
      id: 'perm-2',
      action_type: 'bash',
      tokens: ['echo', 'hello'],
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Execute')).toBeInTheDocument();
    expect(screen.getByText('Permission Required')).toBeInTheDocument();
  });

  it('calls onRespond with allowed=true when Allow clicked', async () => {
    const request: PermRequest = {
      kind: 'permission',
      id: 'perm-3',
      action_type: 'bash',
      tokens: ['rm', '-rf', '/tmp/test'],
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await fireEvent.click(screen.getByText('Allow'));

    expect(mockOnRespond).toHaveBeenCalledWith({
      allowed: true,
      pattern: 'rm -rf /tmp/test',
      scope: 'once',
    });
  });

  it('calls onRespond with allowed=false when Deny clicked', async () => {
    const request: PermRequest = {
      kind: 'permission',
      id: 'perm-4',
      action_type: 'tool',
      tokens: ['dangerous_tool'],
      tool_name: 'exec_sql',
    };

    render(() => <PermissionInteraction request={request} onRespond={mockOnRespond} />);

    await fireEvent.click(screen.getByText('Deny'));

    expect(mockOnRespond).toHaveBeenCalledWith({
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
      action_type: 'read',
      tokens: ['/etc/passwd'],
    };

    render(() => <InteractionHandler request={request} onRespond={mockOnRespond} />);

    expect(screen.getByText('Allow')).toBeInTheDocument();
    expect(screen.getByText('Deny')).toBeInTheDocument();
    expect(screen.getByText('Read')).toBeInTheDocument();
  });
});
