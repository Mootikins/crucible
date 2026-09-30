/**
 * DEV-ONLY shell mockup (model F) on the real windowing core. It is NOT part
 * of the shipped app.
 *
 * Vite serves it in dev at `/shell-mockup.html`. The production rollup input
 * does not name it, so it never reaches `dist/`. The layout, the rails, the
 * tabs, the splits, the floating windows and the expand are the core's; the
 * look comes from a theme stylesheet of `--cru-*` values, as a plugin theme
 * would apply it; the panel bodies are mockups with the docs kiln as content.
 */
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import '@/index.css';
import '@/windowing/theme.css';
import './mockup.css';
import { createEffect } from 'solid-js';
import { render } from 'solid-js/web';
import { configureWindowing, windowActions, windowStore } from '@/windowing/store';
import { WindowManager } from '@/windowing/components/WindowManager';
import type { Tab } from '@/windowing/model/types';
import { mockPolicy, type MockType } from './policy';
import { applyTweaks } from './tweaks';
import { setFocusedNote, setState } from './state';
import { focusComposer, openNote } from './actions';
import { TerminalView } from './components/terminal/TerminalView';
import { ChangesContainer } from './containers/ChangesContainer';
import { FilesPanelContainer } from './containers/FilesPanelContainer';
import { NoteContainer } from './containers/NoteContainer';
import { mockSlots } from './containers/RailContainer';
import { SessionContainer } from './containers/SessionContainer';
import { SessionsPanelContainer } from './containers/SessionsPanelContainer';

configureWindowing(
  mockPolicy((action) => {
    if (action === 'focusComposer') {
      focusComposer();
      return true;
    }
    return false;
  }),
);
Object.assign(window, { __windowStore: windowStore, __windowActions: windowActions });
applyTweaks();

/** The composer's context chip follows the note that has focus in the centre. */
function FocusedNoteTracker() {
  createEffect(() => {
    const paneGroup = windowStore.activePaneId ? windowActions.getPaneTabGroupId(windowStore.activePaneId) : null;
    const g = paneGroup ? windowStore.tabGroups[paneGroup] : undefined;
    const tab = g?.tabs.find((t) => t.id === g.activeTabId);
    if (tab?.contentType === 'note') setFocusedNote(tab.metadata?.path as string);
  });
  return null;
}

/**
 * A session tab that comes to the front makes its session the active one,
 * so the sessions list marks it. A rail icon click activates a tab without
 * the policy hook, so this watches every group's active tab.
 */
function ActiveSessionTracker() {
  let shown = new Set<string>();
  createEffect(() => {
    const now = Object.values(windowStore.tabGroups)
      .map((g) => g.tabs.find((t) => t.id === g.activeTabId))
      .filter((t) => t?.contentType === 'session')
      .map((t) => t!.metadata?.sid as string);
    const fresh = now.find((sid) => !shown.has(sid));
    shown = new Set(now);
    if (fresh) setState('active', fresh);
  });
  return null;
}

const renderContent = (tab: () => Tab) => {
  const t = tab() as Tab<MockType>;
  switch (t.contentType) {
    case 'sessions':
      return <SessionsPanelContainer />;
    case 'files':
      return <FilesPanelContainer />;
    case 'note':
      // Read through `tab()`: a navigation replaces the path in place.
      return <NoteContainer tabId={t.id} path={tab().metadata?.path as string} />;
    case 'changes':
      return <ChangesContainer sid={t.metadata?.sid as string} />;
    case 'session':
      return <SessionContainer sid={t.metadata?.sid as string} />;
    case 'terminal':
      return <TerminalView />;
  }
};

render(
  () => (
    <WindowManager
      renderContent={renderContent}
      slots={mockSlots(focusComposer, () => openNote('Index'))}
    >
      <FocusedNoteTracker />
      <ActiveSessionTracker />
    </WindowManager>
  ),
  document.getElementById('root')!,
);
