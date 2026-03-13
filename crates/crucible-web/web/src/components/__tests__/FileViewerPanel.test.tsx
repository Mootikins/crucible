import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import { registerPanels } from '@/lib/register-panels';

// Mock EditorContext — FileViewerPanel calls useEditorSafe()
vi.mock('@/contexts/EditorContext', () => ({
  useEditorSafe: () => ({
    openFiles: () => [],
    activeFile: () => null,
    openFile: vi.fn(async () => {}),
    closeFile: vi.fn(),
    saveFile: vi.fn(async () => {}),
    setActiveFile: vi.fn(),
    updateFileContent: vi.fn(),
    isLoading: () => false,
    error: () => null,
  }),
}));

// Mock file-actions (used internally by FileViewerPanel for dirty sync)
vi.mock('@/lib/file-actions', () => ({
  findTabByFilePath: vi.fn(() => null),
}));

// Mock windowStore actions (used internally by FileViewerPanel for dirty sync)
vi.mock('@/stores/windowStore', () => ({
  windowActions: {
    updateTab: vi.fn(),
  },
  windowStore: {
    tabGroups: {},
    layout: { id: 'pane-1', type: 'pane', tabGroupId: null },
  },
  setStore: vi.fn(),
}));

// Dynamically import after mocks are in place
const { default: FileViewerPanel } = await import('../FileViewerPanel');

describe('FileViewerPanel — panel registry', () => {
  beforeEach(() => {
    resetGlobalRegistry();
  });

  it('registers "file" content type via registerPanels()', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('file');
    expect(panel).toBeDefined();
    expect(panel!.id).toBe('file');
    expect(panel!.title).toBe('File');
    expect(panel!.defaultZone).toBe('center');
    expect(panel!.icon).toBe('📄');
  });

  it('registers file panel with a valid component', () => {
    registerPanels();
    const panel = getGlobalRegistry().get('file');
    expect(panel).toBeDefined();
    expect(typeof panel!.component).toBe('function');
  });
});

describe('FileViewerPanel — rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders "No file selected" when filePath is undefined', () => {
    render(() => <FileViewerPanel />);
    expect(screen.getByText('No file selected')).toBeInTheDocument();
  });

  it('renders "No file selected" when filePath is not provided', () => {
    render(() => <FileViewerPanel filePath={undefined} />);
    expect(screen.getByText('No file selected')).toBeInTheDocument();
  });

  it('renders loading fallback when filePath is provided but file not yet loaded', () => {
    render(() => <FileViewerPanel filePath="/docs/readme.md" />);
    // With mocked useEditorSafe returning empty openFiles and isLoading=false,
    // the component shows the "Loading file..." fallback inside the <Show> fallback
    expect(screen.getByText('Loading file...')).toBeInTheDocument();
  });
});
