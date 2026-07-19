// Regression: a persisted layout can reference a tabGroupId whose group
// object is missing from `tabGroups` (pre-v3 ghost). addTab used to silently
// no-op on such ids, which made EVERY center open — file-tree click, command
// palette, file drag-onto-pane — do nothing on those layouts.
import { describe, it, expect } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore, windowActions } from '../windowStore';
import type { Tab, TabGroup } from '@/types/windowTypes';

const makeTab = (id: string, title = id): Tab => ({
  id,
  title,
  contentType: 'file',
});

describe('addTab ghost-group self-heal', () => {
  it('materializes a missing group instead of dropping the tab', () => {
    setStore(
      produce((s) => {
        delete (s.tabGroups as Record<string, TabGroup>)['ghost-group'];
      })
    );
    expect(windowStore.tabGroups['ghost-group']).toBeUndefined();

    const tab = makeTab('tab-file-/tmp/a.md', 'a.md');
    windowActions.addTab('ghost-group', tab);

    const group = windowStore.tabGroups['ghost-group'];
    expect(group).toBeDefined();
    expect(group.tabs.map((t) => t.id)).toEqual([tab.id]);
    expect(group.activeTabId).toBe(tab.id);
  });

  it('still appends normally when the group exists', () => {
    setStore(
      produce((s) => {
        (s.tabGroups as Record<string, TabGroup>)['real-group'] = {
          id: 'real-group',
          tabs: [makeTab('t1')],
          activeTabId: 't1',
        };
      })
    );

    windowActions.addTab('real-group', makeTab('t2'));

    const group = windowStore.tabGroups['real-group'];
    expect(group.tabs.map((t) => t.id)).toEqual(['t1', 't2']);
    expect(group.activeTabId).toBe('t2');
  });
});
