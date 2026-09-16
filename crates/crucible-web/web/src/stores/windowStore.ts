import {
  configureWindowing,
  windowStore,
  setStore,
  windowActions,
  findEdgePanelForGroup,
  type WindowActions,
  type WindowPolicy,
} from '@/windowing';
import type { Tab, TabContentType, WindowState } from '@/types/windowTypes';
import { defaultLayout } from './defaultLayout';
import { ensureFixedRails, isLastFixedRailTab } from './fixedRails';
import { appLayoutHooks } from './layoutMigrations';
import { iconForContentType } from '@/lib/tab-icons';
import { terminalAllowed } from '@/lib/terminal-availability';
import { DEFAULT_SHORTCUTS } from '@/lib/keyboard-shortcuts';
import { statusBarActions, statusBarStore } from './statusBarStore';
import { syncShellSurface } from './shellStore';

export type { WindowState } from '@/types/windowTypes';

/** The answers of the app to the questions of the window manager. */
export const appWindowPolicy: WindowPolicy<TabContentType> = {
  seed: defaultLayout,
  // The two rails are fixed: the LAST Sessions panel and the last Files
  // panel do not close.
  mayCloseTab: (s, groupId, tabId) => !isLastFixedRailTab(s, groupId, tabId),
  repairLayout: ensureFixedRails,
  onActiveTabChange: (tab) => {
    // Keep the status bar's "active session" in sync with tab focus so
    // session-scoped commands (Ctrl+K clear, switch-model) hit the chat the
    // user is looking at, not the one that bootstrapped last.
    const sessionId = tab?.metadata?.sessionId;
    if (typeof sessionId === 'string') statusBarActions.setActiveSessionId(sessionId);
    syncShellSurface(tab);
  },
  iconFor: iconForContentType,
  // A terminal needs the host machine, or the remote_shell opt-in.
  unavailableReason: (tab: Tab) =>
    tab.contentType === 'terminal' && !terminalAllowed()
      ? 'only available from the host machine (or with remote_shell enabled)'
      : null,
  layoutHooks: appLayoutHooks,
  shortcuts: DEFAULT_SHORTCUTS,
  onShortcut: (action) => {
    switch (action) {
      case 'focusChatInput':
        document.querySelector<HTMLTextAreaElement>('textarea[data-testid="chat-input"]')?.focus();
        return true;
      case 'newSession':
        window.dispatchEvent(new CustomEvent('crucible:new-session'));
        return true;
      case 'clearChat':
        window.dispatchEvent(new CustomEvent('crucible:clear-chat'));
        return true;
      case 'toggleThinking':
        statusBarActions.setShowThinking(!statusBarStore.showThinking());
        return true;
      default:
        // Only `cycleMode` (Shift+Tab) and `closeOverlay` (Escape) reach this
        // branch: App.tsx handles the palette chords in the capture phase. The
        // composer acts on these two keys on its own element. The `true` keeps
        // the behavior of the old loop, which cancelled the browser default
        // for Shift+Tab and Escape.
        return true;
    }
  },
};

configureWindowing(appWindowPolicy);

// The core store and actions take any content type. Only the app configures
// and writes this store, so `typedStore` and `typedActions` narrow both to the
// app content types, and the narrower types are true here.
const typedStore = windowStore as WindowState;
const typedActions = windowActions as WindowActions<TabContentType>;
export {
  typedStore as windowStore,
  setStore,
  typedActions as windowActions,
  findEdgePanelForGroup,
};

if (typeof window !== 'undefined') {
  (window as unknown as Record<string, unknown>).__windowActions = windowActions;
  (window as unknown as Record<string, unknown>).__windowStore = windowStore;
}
