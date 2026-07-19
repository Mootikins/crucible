import type { Tab } from '@/types/windowTypes';

// Data-loss guard for window-tab close paths (bug 6). Tabs mirror their
// editor dirty state via `isModified` (see FileViewerPanel's dirty-sync
// effect); a modified tab must not close without the user agreeing to
// discard. Returns true when it is OK to proceed with the close.
export function confirmTabClose(tab: Tab): boolean {
  if (!tab.isModified) return true;
  return window.confirm(`Discard unsaved changes to ${tab.title}?`);
}
