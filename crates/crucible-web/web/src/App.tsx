import { Component, createSignal, onCleanup, onMount } from 'solid-js';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { WindowManager } from '@/components/windowing/WindowManager';
import { CommandPalette, type PaletteCommand } from '@/components/CommandPalette';
import { registerPanels } from '@/lib/register-panels';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '@/lib/layout-persistence';
import { matchShortcut } from '@/lib/keyboard-shortcuts';
import { statusBarActions, statusBarStore } from '@/stores/statusBarStore';
import { windowActions } from '@/stores/windowStore';
import { NotificationToast } from '@/components/NotificationToast';
import { ExportDialog } from '@/components/ExportDialog';

function focusChatInput(): void {
  const candidate = document.querySelector<HTMLTextAreaElement | HTMLInputElement | HTMLElement>(
    'textarea, input[type="text"], [contenteditable="true"]'
  );
  if (!candidate) return;
  candidate.focus();
}

function openSettingsPanel(): void {
  windowActions.setEdgePanelCollapsed('right', false);
  windowActions.setEdgePanelActiveTab('right', 'outline-tab');
}

function openFilesPanel(): void {
  windowActions.setEdgePanelCollapsed('left', false);
  windowActions.setEdgePanelActiveTab('left', 'explorer-tab');
}

const App: Component = () => {
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = createSignal(false);
  const [isExportDialogOpen, setIsExportDialogOpen] = createSignal(false);

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
      id: 'nav-open-settings',
      label: 'Open Settings',
      description: 'Show settings in the right side panel.',
      category: 'Navigation',
      keywords: ['open', 'settings', 'panel'],
      action: openSettingsPanel,
    },
    {
      id: 'nav-open-files',
      label: 'Open Files',
      description: 'Reveal the explorer in the left panel.',
      category: 'Navigation',
      keywords: ['files', 'explorer', 'left panel'],
      action: openFilesPanel,
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
    {
      id: 'settings-open-panel',
      label: 'Open Settings Panel',
      description: 'Jump to settings controls quickly.',
      category: 'Settings',
      keywords: ['settings', 'preferences', 'config'],
      action: openSettingsPanel,
    },
  ];

  onMount(() => {
    registerPanels();
    loadLayoutOnStartup();
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

    onCleanup(() => {
      document.removeEventListener('keydown', onGlobalKeyDown, true);
      window.removeEventListener('crucible:export-session', onExportSession);
    });
  });

  return (
    <SettingsProvider>
      <WindowManager />
      <NotificationToast />
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
    </SettingsProvider>
  );
};

export default App;
