import { Component, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider } from '@/contexts/SessionContext';
import { EditorProvider } from '@/contexts/EditorContext';
import { AppShell } from '@/components/AppShell';
import { CommandPalette, type PaletteCommand, type PaletteMode } from '@/components/CommandPalette';
import { shellActions } from '@/stores/shellStore';
import { registerPanels } from '@/lib/register-panels';
import { getGlobalRegistry } from '@/lib/panel-registry';
import type { TabContentType } from '@/types/windowTypes';
import { useConfig } from '@/lib/query/config';
import { markShell, startLayoutPersistence } from '@/lib/shell-boot';
import { isCompact } from '@/stores/deviceStore';
import { matchShortcut } from '@/windowing';
import { DEFAULT_SHORTCUTS } from '@/lib/keyboard-shortcuts';
import { openSessionInChat } from '@/lib/session-actions';
import { openDraftSession } from '@/lib/draft-session';
import { openFileInEditor } from '@/lib/file-actions';
import { openPanelTab } from '@/lib/panel-actions';
import { terminalAllowed } from '@/lib/terminal-availability';
import { statusBarActions, statusBarStore } from '@/stores/statusBarStore';
import { attentionActions } from '@/stores/attentionStore';
import { windowActions } from '@/stores/windowStore';
import { NotificationToast } from '@/components/NotificationToast';
import { ExportDialog } from '@/components/ExportDialog';
import { SettingsModal } from '@/components/settings/SettingsModal';
import { AuthTokenPrompt } from '@/components/AuthTokenPrompt';
import { getBus } from '@/lib/bus';

function focusChatInput(): void {
  const candidate = document.querySelector<HTMLTextAreaElement | HTMLInputElement | HTMLElement>(
    'textarea, input[type="text"], [contenteditable="true"]'
  );
  if (!candidate) return;
  candidate.focus();
}

/** Content types that only make sense with a target (a specific file or
 * session) — they get no generic "Open …" palette command. */
// Settings is absent because it is not a panel at all: the registry never
// holds it, and the explicit "Settings" command below opens the dialog.
const PANEL_COMMAND_EXCLUDED = new Set<string>(['file', 'chat', 'chat-draft']);

/** Panel-specific palette descriptions; anything unlisted gets a generic one. */
const PANEL_COMMAND_DESCRIPTIONS: Record<string, string> = {
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
    // No "Open Terminal" on clients that can't use it (remote without the
    // remote_shell opt-in) — the command would open an explanation panel.
    .filter((def) => def.id !== 'terminal' || terminalAllowed())
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
  const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
  // The shared config query, not a fetch of its own: every other surface that
  // wants the default kiln reads the same key, so the shell asks once.
  const config = useConfig();
  const kilnPath = () => config.data?.kiln_path;

  // Seed the shell header/status bar before any session is selected. It runs
  // in an effect rather than in `onMount` because the answer can arrive after
  // the mount, and a seeded bar must not be overwritten.
  createEffect(() => {
    const path = kilnPath();
    if (path === undefined) return;
    if (!statusBarStore.kilnPath()) statusBarActions.setKilnPath(path ?? null);
  });

  const paletteCommands: PaletteCommand[] = [
    {
      id: 'chat-new-session',
      label: 'New Chat Session',
      description: 'Start a fresh chat session.',
      shortcut: 'Ctrl+Shift+N',
      category: 'Chat',
      keywords: ['new', 'session', 'chat'],
      action: () => getBus().emit('newSession', {}),
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
      id: 'open-settings',
      label: 'Settings',
      description: 'Appearance, editor, model, plugins and workspace.',
      category: 'Settings',
      keywords: ['settings', 'preferences', 'options', 'config', 'theme', 'font'],
      action: () => setIsSettingsOpen(true),
    },    {
      id: 'files-toggle-hidden',
      label: 'Toggle Hidden Files',
      description: 'Show or hide dotfiles in the file tree.',
      category: 'Navigation',
      keywords: ['hidden', 'dotfiles', 'files', 'tree', 'show'],
      action: () => window.dispatchEvent(new CustomEvent('crucible:toggle-hidden-files')),
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
      id: 'nav-swap-sides',
      label: 'Swap Side Panels',
      description: 'Mirror the left and right panels — put the file tree on your dominant side.',
      shortcut: 'Ctrl+Shift+\\',
      category: 'Navigation',
      keywords: ['swap', 'mirror', 'flip', 'sides', 'panel'],
      action: () => windowActions.swapSidePanels(),
    },
  ];

  onMount(() => {
    // Poll the daemon's pending-interaction aggregate so the Inbox badge
    // covers sessions without an open tab (WS-302).
    const stopAttentionPolling = attentionActions.startPolling();

    // No landing page: a fresh shell (no persisted center content) shows the
    // center composer — context chips + first-message box + quick actions.
    // Users build their own home from panels. The compact shell has no layout,
    // and must never save one: see `startLayoutPersistence`.
    startLayoutPersistence({ compact: isCompact() });
    // The stylesheet's compact rules read this, rather than a live media
    // query that would disagree with the shell on a narrowed desktop window.
    markShell(isCompact());

    const onGlobalKeyDown = (event: KeyboardEvent) => {
      if (isCommandPaletteOpen() && event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        setIsCommandPaletteOpen(false);
        return;
      }

      const action = matchShortcut(event, DEFAULT_SHORTCUTS);
      if (action === 'openCommandPalette') {
        event.preventDefault();
        event.stopPropagation();
        openPalette();
      } else if (action === 'openNoteSwitcher') {
        event.preventDefault();
        event.stopPropagation();
        openPalette('notes');
      } else if (action === 'openSearch') {
        event.preventDefault();
        event.stopPropagation();
        openPanelTab('search');
        // Panel focuses its input on mount; re-focus if it was already open.
        window.dispatchEvent(new CustomEvent('crucible:focus-search'));
      }
    };

    document.addEventListener('keydown', onGlobalKeyDown, true);

    const onExportSession = () => setIsExportDialogOpen(true);
    getBus().on('openSettings', () => setIsSettingsOpen(true));
    window.addEventListener('crucible:export-session', onExportSession);
    // Every new-session entry point (ribbon, Home, palette, empty states)
    // opens the draft surface; the session is created lazily on first send.
    // `workspace` names the project the session acts in — the sessions
    // tree's per-project New Session row sends it. Absent means "unset", which
    // the composer leaves for the user to pick.
    getBus().on('newSession', ({ workspace }) => openDraftSession({ workspace }));
    getBus().on('openSession', ({ sessionId, title }) => openSessionInChat(sessionId, title));
    // Open a kiln file in the editor programmatically (symmetric with
    // open-session). Lets other panels/commands "reveal in editor" a path
    // without a sidebar click.
    getBus().on('openFile', ({ path, name }) =>
      openFileInEditor(path, name ?? path.split('/').pop() ?? path));
    // Ribbon palette button (WindowManager can't reach the palette signal).
    // `mode` lets non-App surfaces (center composer CTAs) open the notes tree
    // directly instead of the commands list.
    getBus().on('openCommandPalette', ({ mode }) => openPalette(mode ?? 'commands'));

    onCleanup(() => {
      stopAttentionPolling();
      document.removeEventListener('keydown', onGlobalKeyDown, true);
      window.removeEventListener('crucible:export-session', onExportSession);
    });
  });

  return (
    <SettingsProvider>
      <WhisperProvider>
        <ProjectProvider>
          <SessionProvider initialKiln={kilnPath()}>
            <EditorProvider>
              <AppShell />
            </EditorProvider>
            <NotificationToast />
            {/* WikilinkHoverPreview mounts inside WindowManager's DnD provider
                so hover cards can drag file tabs into panes/panels. */}
            <AuthTokenPrompt />
            <SettingsModal open={isSettingsOpen()} onClose={() => setIsSettingsOpen(false)} />
            <ExportDialog
              open={isExportDialogOpen()}
              sessionId={statusBarStore.activeSessionId()}
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
      </WhisperProvider>
    </SettingsProvider>
  );

};

export default App;
