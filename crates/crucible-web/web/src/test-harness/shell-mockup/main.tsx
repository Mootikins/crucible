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
import './mockup.css';
import { createEffect } from 'solid-js';
import { render } from 'solid-js/web';
import { configureWindowing, windowActions, windowStore } from '@/windowing/store';
import { WindowManager } from '@/windowing/components/WindowManager';
import type { Tab } from '@/windowing/model/types';
import { mockPolicy, type MockType } from './policy';
import { mockSlots } from './chrome';
import { ChangesView, FilesPanel, NoteView, SessionView, SessionsPanel, TerminalView } from './panels';
import { setFocusedNote } from './state';
import { openNote } from './actions';

configureWindowing(
  mockPolicy((action) => {
    if (action === 'focusComposer') {
      document.querySelector<HTMLTextAreaElement>('.mk-composer textarea')?.focus();
      return true;
    }
    return false;
  }),
);
Object.assign(window, { __windowStore: windowStore, __windowActions: windowActions });

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

const renderContent = (tab: () => Tab) => {
  const t = tab() as Tab<MockType>;
  switch (t.contentType) {
    case 'sessions':
      return <SessionsPanel />;
    case 'files':
      return <FilesPanel />;
    case 'note':
      return <NoteView path={t.metadata?.path as string} />;
    case 'changes':
      return <ChangesView sid={t.metadata?.sid as string} />;
    case 'session':
      return <SessionView />;
    case 'terminal':
      return <TerminalView />;
  }
};

render(
  () => (
    <WindowManager
      renderContent={renderContent}
      slots={mockSlots(
        () => document.querySelector<HTMLTextAreaElement>('.mk-composer textarea')?.focus(),
        () => openNote('Index'),
      )}
    >
      <FocusedNoteTracker />
    </WindowManager>
  ),
  document.getElementById('root')!,
);
