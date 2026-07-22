import { Component, createSignal, onCleanup, onMount } from 'solid-js';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider } from '@/contexts/SessionContext';
import { EditorProvider } from '@/contexts/EditorContext';
import { WindowManager } from '@/components/windowing/WindowManager';
import { CommandPalette, type PaletteCommand, type PaletteMode } from '@/components/CommandPalette';
import { shellActions } from '@/stores/shellStore';
import { registerPanels } from '@/lib/register-panels';
import { getGlobalRegistry } from '@/lib/panel-registry';
import type { TabContentType } from '@/types/windowTypes';
import { getConfig } from '@/lib/api';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '@/lib/layout-persistence';
import { matchShortcut } from '@/lib/keyboard-shortcuts';
import { openSessionInChat } from '@/lib/session-actions';
import { openDraftSession } from '@/lib/draft-session';
import { openFileInEditor } from '@/lib/file-actions';
import { openPanelTab } from '@/lib/panel-actions';
import { statusBarActions, statusBarStore } from '@/stores/statusBarStore';
import { attentionActions } from '@/stores/attentionStore';
import { windowActions } from '@/stores/windowStore';
import { NotificationToast } from '@/components/NotificationToast';
import { ExportDialog } from '@/components/ExportDialog';
import { AuthTokenPrompt } from '@/components/AuthTokenPrompt';

function focusChatInput(): void {
  const candidate = document.querySelector<HTMLTextAreaElement | HTMLInputElement | HTMLElement>(
    'textarea, input[type="text"], [contenteditable="true"]'
  );
  if (!candidate) return;
  candidate.focus();
}

/** Content types that only make sense with a target (a specific file or
 * session) — they get no generic "Open …" palette command. */
const PANEL_COMMAND_EXCLUDED = new Set<string>(['file', 'chat', 'chat-draft']);

/** Panel-specific palette descriptions; anything unlisted gets a generic one. */
const PANEL_COMMAND_DESCRIPTIONS: Record<string, string> = {
  settings: 'Open the settings tab.',
  files: 'Browse workspace files and kiln notes.',
  plugins: 'Manage installed plugins.',
  skills: 'Browse and search agent skills.',
  backlinks: 'Linked and unlinked mentions for the focused note.',
  graph: 'Interactive knowledge graph of the kiln.',
  terminal: 'Shell terminal in the bottom panel.',
  sessions: 'Session list in the left panel.',
  inbox: 'Everything waiting on you, one place.',
  activity: 'Live agent activity feed.',
};

/** One "Open …" command per registered panel, so any closed window can be
 * brought back from the palette (focuses the existing tab if still open). */
function panelOpenCommands(): PaletteCommand[] {
  return getGlobalRegistry()
    .list()
    .filter((def) => !PANEL_COMMAND_EXCLUDED.has(def.id))
    .map((def) => ({
      id: `nav-open-${def.id}`,
      label: `Open ${def.title}`,
      description: PANEL_COMMAND_DESCRIPTIONS[def.id] ?? `Open the ${def.title} panel.`,
      category: 'Navigation' as const,
      keywords: ['open', 'panel', 'window', 'reopen', 'view', def.id],
      action: () => openPanelTab(def.id as TabContentType),
    }));
}

const App: Component = () => {
  registerPanels();
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = createSignal(false);
  // Ctrl+P opens the palette in commands mode, Ctrl+O in notes mode.
  const [paletteMode, setPaletteMode] = createSignal<PaletteMode>('commands');
  const openPalette = (mode: PaletteMode = 'commands') => {
    setPaletteMode(mode);
    setIsCommandPaletteOpen(true);
  };
  const [isExportDialogOpen, setIsExportDialogOpen] = createSignal(false);
  const [kilnPath, setKilnPath] = createSignal<string | undefined>(undefined);

  const paletteCommands: PaletteCommand[] = [
    {
      id: 'chat-new-session',
      label: 'New Chat Session',
      description: 'Start a fresh chat session.',
      shortcut: 'Ctrl+Shift+N',
      category: 'Chat',
      keywords: ['new', 'session', 'chat'],
      action: () => window.dispatchEvent(new CustomEvent('crucible:new-session')),
    },
    {
      id: 'chat-clear',
      label: 'Clear Chat',
      description: 'Clear visible chat messages.',
      shortcut: 'Ctrl+K',
      category: 'Chat',
      keywords: ['clear', 'chat', 'messages'],
      action: () => window.dispatchEvent(new CustomEvent('crucible:clear-chat')),
    },
    {
      id: 'chat-focus-input',
      label: 'Focus Chat Input',
      description: 'Move cursor focus to the chat composer.',
      shortcut: 'Ctrl+/',
      category: 'Chat',
      keywords: ['focus', 'input', 'composer'],
      action: focusChatInput,
    },
    {
      id: 'chat-toggle-thinking',
      label: 'Toggle Thinking Display',
      description: 'Show or hide assistant thinking blocks.',
      shortcut: 'Ctrl+T',
      category: 'Chat',
      keywords: ['thinking', 'reasoning', 'toggle'],
      action: () => statusBarActions.setShowThinking(!statusBarStore.showThinking()),
    },
    {
      id: 'session-export',
      label: 'Export Session',
      description: 'Export current session to markdown.',
      category: 'Session',
      keywords: ['export', 'session', 'markdown'],
      action: () => setIsExportDialogOpen(true),
    },
    {
      id: 'session-switch-model',
      label: 'Switch Model',
      description: 'Open model switcher for this session.',
      category: 'Session',
      keywords: ['model', 'llm', 'switch'],
      action: () => window.dispatchEvent(new CustomEvent('crucible:switch-model')),
    },
    {
      id: 'session-search',
      label: 'Search Sessions',
      description: 'Find sessions by title or content.',
      category: 'Session',
      keywords: ['search', 'find', 'session', 'filter'],
      action: () => {
        windowActions.setEdgePanelCollapsed('left', false);
        // Defer to next tick so the panel is visible before focusing
        setTimeout(() => window.dispatchEvent(new CustomEvent('crucible:focus-session-search')), 100);
      },
    },
    {
      id: 'nav-open-note',
      label: 'Open Note…',
      description: 'Quick switcher: jump to a note by name.',
      shortcut: 'Ctrl+O',
      category: 'Navigation',
      keywords: ['note', 'quick', 'switcher', 'jump', 'file'],
      // Selecting a command closes the palette after the action runs; defer
      // the mode-switched reopen so it lands after that close.
      action: () => setTimeout(() => openPalette('notes'), 0),
    },
    {
      id: 'nav-go-edit',
      label: 'Go to Editor',
      description: 'Focus the most recent file tab (opens the notes tree if none).',
      category: 'Navigation',
      keywords: ['edit', 'editor', 'vault', 'notes', 'go'],
      action: () => shellActions.goEdit(),
    },
    {
      id: 'nav-go-session',
      label: 'Go to Session',
      description: 'Focus the active session chat (starts one if none).',
      category: 'Navigation',
      keywords: ['chat', 'session', 'agent', 'go'],
      action: () => shellActions.goSession(),
    },
    // Every registered panel gets an "Open …" command — the way to bring
    // back a closed window (graph, terminal, backlinks…). Focuses the
    // existing tab when one is already open. Content-parameterized types
    // (file/chat) are excluded: they need a target, not a singleton tab.
    ...panelOpenCommands(),
    {
      id: 'nav-toggle-left',
      label: 'Toggle Left Panel',
      description: 'Collapse or expand the left edge panel.',
      shortcut: 'Ctrl+B',
      category: 'Navigation',
      keywords: ['toggle', 'left', 'panel'],
      action: () => windowActions.toggleEdgePanel('left'),
    },
    {
      id: 'nav-toggle-right',
      label: 'Toggle Right Panel',
      description: 'Collapse or expand the right edge panel.',
      shortcut: 'Ctrl+Shift+E',
      category: 'Navigation',
      keywords: ['toggle', 'right', 'panel'],
      action: () => windowActions.toggleEdgePanel('right'),
    },
    {
      id: 'nav-toggle-bottom',
      label: 'Toggle Bottom Panel',
      description: 'Collapse or expand the bottom edge panel.',
      shortcut: 'Ctrl+Shift+B',
      category: 'Navigation',
      keywords: ['toggle', 'bottom', 'terminal', 'panel'],
      action: () => windowActions.toggleEdgePanel('bottom'),
    },
  ];

  onMount(() => {
    getConfig()
      .then((cfg) => {
        setKilnPath(cfg.kiln_path);
        // Seed the shell header/status bar before any session is selected.
        if (!statusBarStore.kilnPath()) {
          statusBarActions.setKilnPath(cfg.kiln_path ?? null);
        }
      })
      .catch(() => {});

    // Poll the daemon's pending-interaction aggregate so the Inbox badge
    // covers sessions without an open tab (WS-302).
    const stopAttentionPolling = attentionActions.startPolling();

    // No landing page: a fresh shell (no persisted center content) shows the
    // center pane's EmptyState, whose action starts a new session. Users
    // build their own home from panels.
    void loadLayoutOnStartup();
    setupLayoutAutoSave();

    const onGlobalKeyDown = (event: KeyboardEvent) => {
      if (isCommandPaletteOpen() && event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        setIsCommandPaletteOpen(false);
        return;
      }

      const action = matchShortcut(event);
      if (action === 'openCommandPalette') {
        event.preventDefault();
        event.stopPropagation();
        openPalette();
      } else if (action === 'openNoteSwitcher') {
        event.preventDefault();
        event.stopPropagation();
        openPalette('notes');
      }
    };

    document.addEventListener('keydown', onGlobalKeyDown, true);

    // Listen for export-session custom event (dispatched from command palette or other sources)
    const onExportSession = () => setIsExportDialogOpen(true);
    window.addEventListener('crucible:export-session', onExportSession);
    // Every new-session entry point (ribbon, Home, palette, empty states)
    // opens the draft surface; the session is created lazily on first send.
    const onNewSession = () => openDraftSession();
    window.addEventListener('crucible:new-session', onNewSession);
    const onOpenSession = (e: Event) => {
      const { sessionId, title } = (e as CustomEvent<{ sessionId: string; title: string }>).detail;
      openSessionInChat(sessionId, title);
    };
    window.addEventListener('crucible:open-session', onOpenSession);
    // Open a kiln file in the editor programmatically (symmetric with
    // open-session). Lets other panels/commands "reveal in editor" a path
    // without a sidebar click.
    const onOpenFile = (e: Event) => {
      const { path, name } = (e as CustomEvent<{ path: string; name?: string }>).detail;
      openFileInEditor(path, name ?? path.split('/').pop() ?? path);
    };
    window.addEventListener('crucible:open-file', onOpenFile);
    // Ribbon palette button (WindowManager can't reach the palette signal).
    const onOpenPalette = () => openPalette();
    window.addEventListener('crucible:open-command-palette', onOpenPalette);

    onCleanup(() => {
      stopAttentionPolling();
      document.removeEventListener('keydown', onGlobalKeyDown, true);
      window.removeEventListener('crucible:export-session', onExportSession);
      window.removeEventListener('crucible:new-session', onNewSession);
      window.removeEventListener('crucible:open-session', onOpenSession);
      window.removeEventListener('crucible:open-file', onOpenFile);
      window.removeEventListener('crucible:open-command-palette', onOpenPalette);
    });
  });

  return (
    <SettingsProvider>
      <ProjectProvider>
        <SessionProvider initialKiln={kilnPath()}>
          <EditorProvider>
            <WindowManager />
          </EditorProvider>
          <NotificationToast />
          {/* WikilinkHoverPreview mounts inside WindowManager's DnD provider
              so hover cards can drag file tabs into panes/panels. */}
          <AuthTokenPrompt />
          <ExportDialog
            open={isExportDialogOpen()}
            sessionId={statusBarStore.activeSessionId()}
            sessionTitle={statusBarStore.activeSessionTitle()}
            onClose={() => setIsExportDialogOpen(false)}
          />
          <CommandPalette
            open={isCommandPaletteOpen()}
            commands={paletteCommands}
            mode={paletteMode()}
            onOpenChange={setIsCommandPaletteOpen}
          />
        </SessionProvider>
      </ProjectProvider>
    </SettingsProvider>
  );

};

export default App;
