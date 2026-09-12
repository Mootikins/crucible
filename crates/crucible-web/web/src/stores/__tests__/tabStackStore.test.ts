import { describe, it, expect, beforeEach } from 'vitest';
import {
  COMPACT_TABS_KEY,
  tabStack,
  tabStackActions,
  loadCompactTabs,
} from '@/stores/tabStackStore';
import type { Tab } from '@/types/windowTypes';

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
});

describe('tabStackStore', () => {
  it('opens a tab and makes it active', () => {
    tabStackActions.open(tab('a'));
    expect(tabStack.tabs.map((t) => t.id)).toEqual(['a']);
    expect(tabStackActions.activeTab()?.id).toBe('a');
  });

  it('focuses an already-open tab instead of opening it twice', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.open(tab('b'));
    tabStackActions.open(tab('a'));
    expect(tabStack.tabs.map((t) => t.id)).toEqual(['a', 'b']);
    expect(tabStackActions.activeTab()?.id).toBe('a');
  });

  it('patches a tab without replacing the rest', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.update('a', { isModified: true });
    expect(tabStackActions.activeTab()?.isModified).toBe(true);
    expect(tabStackActions.activeTab()?.title).toBe('a');
  });

  it('activates the previously visited tab when one closes', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.open(tab('b'));
    tabStackActions.remove('b');
    expect(tabStackActions.activeTab()?.id).toBe('a');
  });

  it('has no active tab once the last one closes', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.remove('a');
    expect(tabStackActions.activeTab()).toBeNull();
    expect(tabStack.tabs).toHaveLength(0);
  });

  // Back walks where the user has BEEN, not the order tabs were opened.
  it('walks back through visit order, each tab once, then gives up', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.open(tab('b'));
    tabStackActions.open(tab('c'));
    tabStackActions.activate('a');

    expect(tabStackActions.back()).toBe(true);
    expect(tabStackActions.activeTab()?.id).toBe('c');
    expect(tabStackActions.back()).toBe(true);
    expect(tabStackActions.activeTab()?.id).toBe('b');
    // Every tab visited once: the next back belongs to the browser.
    expect(tabStackActions.back()).toBe(false);
  });

  it('starts a fresh back walk after the user picks a tab', () => {
    tabStackActions.open(tab('a'));
    tabStackActions.open(tab('b'));
    expect(tabStackActions.back()).toBe(true);
    tabStackActions.activate('b');
    expect(tabStackActions.back()).toBe(true);
  });

  it('persists the open tabs for the next visit', () => {
    tabStackActions.open(tab('a', { title: 'Note A' }));
    tabStackActions.open(tab('b'));
    const stored = JSON.parse(localStorage.getItem(COMPACT_TABS_KEY)!);
    expect(stored.tabs.map((t: Tab) => t.id)).toEqual(['a', 'b']);
    expect(stored.activeTabId).toBe('b');
  });

  // An icon is a component, so it cannot survive JSON. The content type can,
  // and the icon comes back from it.
  it('restores tabs with their icons rebuilt from the content type', () => {
    tabStackActions.open(tab('a'));
    const restored = loadCompactTabs();
    expect(restored.tabs[0].title).toBe('a');
    expect(restored.tabs[0].metadata).toEqual({ filePath: '/kiln/a.md' });
    expect(typeof restored.tabs[0].icon).toBe('function');
  });

  it('survives storage that holds nonsense', () => {
    localStorage.setItem(COMPACT_TABS_KEY, '{not json');
    expect(loadCompactTabs()).toEqual({ tabs: [], activeTabId: null });
  });

  it('drops a restored tab whose content type no longer exists', () => {
    localStorage.setItem(
      COMPACT_TABS_KEY,
      JSON.stringify({ tabs: [{ id: 'x', title: 'x', contentType: 'nonesuch' }], activeTabId: 'x' }),
    );
    const restored = loadCompactTabs();
    expect(restored.tabs).toHaveLength(0);
    expect(restored.activeTabId).toBeNull();
  });
});
