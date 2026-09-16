import type { Tab } from './types';

// A tab with `isModified` holds changes that are not saved. The tab must not
// close until the user agrees to lose the changes. The function returns true
// when the close can go on.
export function confirmTabClose(tab: Tab): boolean {
  if (!tab.isModified) return true;
  return window.confirm(`Discard unsaved changes to ${tab.title}?`);
}
