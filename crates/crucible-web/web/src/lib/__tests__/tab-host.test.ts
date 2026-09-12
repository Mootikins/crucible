import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Tab } from '@/types/windowTypes';

const device = vi.hoisted(() => ({ compact: false }));
vi.mock('@/stores/deviceStore', () => ({ isCompact: () => device.compact }));

import { tabHost } from '@/lib/tab-host';
import { tabStackActions, tabStack } from '@/stores/tabStackStore';
import { windowActions, windowStore } from '@/stores/windowStore';

const tab = (id: string, over: Partial<Tab> = {}): Tab => ({
  id,
  title: id,
  contentType: 'file',
  metadata: { filePath: `/kiln/${id}.md` },
  ...over,
});

beforeEach(() => {
  localStorage.clear();
  tabStackActions.reset();
  device.compact = false;
});

describe('tabHost', () => {
  it('picks the store of the shell this page drew', () => {
    device.compact = true;
    const compactHost = tabHost();
    device.compact = false;
    expect(tabHost()).not.toBe(compactHost);
  });

  describe('on the compact shell', () => {
    beforeEach(() => {
      device.compact = true;
    });

    it('opens, finds, updates and removes through the flat stack', () => {
      const host = tabHost();
      expect(host.open(tab('a'))).toBe(true);
      expect(host.find((t) => t.metadata?.filePath === '/kiln/a.md')?.id).toBe('a');
      expect(host.activeTab()?.id).toBe('a');

      host.update('a', { isModified: true });
      expect(host.find((t) => t.id === 'a')?.isModified).toBe(true);

      host.remove('a');
      expect(host.list()).toHaveLength(0);
      expect(host.activeTab()).toBeNull();
    });

    // Placement describes a desktop layout the phone does not have.
    it('ignores placement, because there is nowhere else to put a tab', () => {
      const host = tabHost();
      host.open(tab('a'), { placement: 'beside-editor' });
      host.open(tab('b'), { placement: 'zone' });
      expect(host.list().map((t) => t.id)).toEqual(['a', 'b']);
      expect(tabStack.activeTabId).toBe('b');
    });

    it('writes nothing to the desktop store', () => {
      tabHost().open(tab('a'));
      const desktopTabs = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs);
      expect(desktopTabs.some((t) => t.id === 'a')).toBe(false);
    });
  });

  describe('on the desktop shell', () => {
    it('finds and updates a tab the window store holds', () => {
      const groupId = Object.keys(windowStore.tabGroups)[0];
      windowActions.addTab(groupId, tab('desk-1'));

      const host = tabHost();
      expect(host.find((t) => t.id === 'desk-1')?.title).toBe('desk-1');
      host.update('desk-1', { title: 'renamed' });
      expect(host.find((t) => t.id === 'desk-1')?.title).toBe('renamed');

      host.remove('desk-1');
      expect(host.find((t) => t.id === 'desk-1')).toBeNull();
    });

    it('writes nothing to the compact stack', () => {
      const groupId = Object.keys(windowStore.tabGroups)[0];
      tabHost().open(tab('desk-2'), { placement: 'editor' });
      expect(tabStack.tabs).toHaveLength(0);
      windowActions.removeTab(groupId, 'desk-2');
    });
  });
});
