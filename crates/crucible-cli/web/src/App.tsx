import { Component, createSignal, onCleanup, onMount } from 'solid-js';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider } from '@/contexts/SessionContext';
import { EditorProvider } from '@/contexts/EditorContext';
import { WindowManager } from '@/components/windowing/WindowManager';
import { CommandPalette, type PaletteCommand } from '@/components/CommandPalette';
import { registerPanels } from '@/lib/register-panels';
import { getConfig } from '@/lib/api';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '@/lib/layout-persistence';
import { matchShortcut } from '@/lib/keyboard-shortcuts';
import { openSessionInChat } from '@/lib/session-actions';
import { openFileInEditor } from '@/lib/file-actions';
import { openPanelTab, findFirstCenterPaneGroupId } from '@/lib/panel-actions';
import { statusBarActions, statusBarStore } from '@/stores/statusBarStore';
import { attentionActions } from '@/stores/attentionStore';
import { windowActions, windowStore } from '@/stores/windowStore';
import { NotificationToast } from '@/components/NotificationToast';
import { WikilinkHoverPreview } from '@/components/WikilinkHoverPreview';
import { ExportDialog } from '@/components/ExportDialog';
import { AuthTokenPrompt } from '@/components/AuthTokenPrompt';

function focusChatInput(): void {
  const candidate = document.querySelector<HTMLTextAreaElement | HTMLInputElement | HTMLElement>(
    'textarea, input[type="text"], [contenteditable="true"]'
  );
  if (!candidate) return;
  candidate.focus();
}

function openSettingsPanel(): void {
  openPanelTab('settings');
}

function openFilesPanel(): void {
  openPanelTab('files');
}

const App: Component = () => {
  registerPanels();
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = createSignal(false);
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
      id: 'nav-open-settings',
      label: 'Open Settings',
      description: 'Open the settings tab.',
      category: 'Navigation',
      keywords: ['open', 'settings', 'panel'],
      action: openSettingsPanel,
    },
    {
      id: 'nav-open-files',
      label: 'Open Files',
      description: 'Browse workspace files and kiln notes.',
      category: 'Navigation',
      keywords: ['files', 'explorer', 'left panel'],
      action: openFilesPanel,
    },
    {
      id: 'nav-open-plugins',
      label: 'Open Plugins',
      description: 'Manage installed plugins.',
      category: 'Navigation',
      keywords: ['plugins', 'install', 'reload', 'panel'],
      action: () => openPanelTab('plugins'),
    },
    {
      id: 'nav-open-skills',
      label: 'Open Skills',
      description: 'Browse and search agent skills.',
      category: 'Navigation',
      keywords: ['skills', 'browse', 'panel'],
      action: () => openPanelTab('skills'),
    },
    {
      id: 'nav-open-backlinks',
      label: 'Open Backlinks',
      description: 'Linked and unlinked mentions for the focused note.',
      category: 'Navigation',
      keywords: ['backlinks', 'mentions', 'wikilinks', 'panel'],
      action: () => openPanelTab('backlinks'),
    },
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

    // Land on Home when the restored layout has no center content — the
    // shell always has somewhere to start (Crucible Shell design turn 5).
    void loadLayoutOnStartup().then(() => {
      const groupId = findFirstCenterPaneGroupId();
      const group = groupId ? windowStore.tabGroups[groupId] : null;
      if (!group || group.tabs.length === 0) {
        openPanelTab('home');
      }
    });
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
        setIsCommandPaletteOpen(true);
      }
    };

    document.addEventListener('keydown', onGlobalKeyDown, true);

    // Listen for export-session custom event (dispatched from command palette or other sources)
    const onExportSession = () => setIsExportDialogOpen(true);
    window.addEventListener('crucible:export-session', onExportSession);
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
    // Header-bar palette pill (WindowManager can't reach the palette signal).
    const onOpenPalette = () => setIsCommandPaletteOpen(true);
    window.addEventListener('crucible:open-command-palette', onOpenPalette);

    onCleanup(() => {
      stopAttentionPolling();
      document.removeEventListener('keydown', onGlobalKeyDown, true);
      window.removeEventListener('crucible:export-session', onExportSession);
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
          <WikilinkHoverPreview />
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
            onOpenChange={setIsCommandPaletteOpen}
          />
        </SessionProvider>
      </ProjectProvider>
    </SettingsProvider>
  );

};

export default App;
