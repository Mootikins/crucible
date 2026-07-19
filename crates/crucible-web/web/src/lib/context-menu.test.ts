// src/lib/context-menu.test.ts
import { describe, it, expect } from 'vitest';
import { shouldUseNativeMenu, tabsToClose } from './context-menu';

const tab = (id: string) => ({ id });

describe('tabsToClose', () => {
  const tabs = [tab('a'), tab('b'), tab('c'), tab('d')];

  it('close removes exactly the target', () => {
    expect(tabsToClose(tabs, 'b', 'close').map((t) => t.id)).toEqual(['b']);
  });
  it('close-others keeps only the target', () => {
    expect(tabsToClose(tabs, 'b', 'close-others').map((t) => t.id)).toEqual(['a', 'c', 'd']);
  });
  it('close-right removes everything after the target', () => {
    expect(tabsToClose(tabs, 'b', 'close-right').map((t) => t.id)).toEqual(['c', 'd']);
    expect(tabsToClose(tabs, 'd', 'close-right')).toEqual([]);
  });
  it('unknown tab closes nothing', () => {
    expect(tabsToClose(tabs, 'zz', 'close-others')).toEqual([]);
  });
});

describe('shouldUseNativeMenu', () => {
  const eventOn = (el: Element, shiftKey = false): MouseEvent => {
    const e = new MouseEvent('contextmenu', { shiftKey, bubbles: true });
    Object.defineProperty(e, 'target', { value: el });
    return e;
  };

  it('shift+right-click is always native', () => {
    const div = document.createElement('div');
    expect(shouldUseNativeMenu(eventOn(div, true))).toBe(true);
  });

  it('images and external links keep the native menu (copy/save must work)', () => {
    const wrap = document.createElement('div');
    wrap.innerHTML = '<figure><img alt=""></figure><a href="https://x.example">x</a><span>text</span>';
    expect(shouldUseNativeMenu(eventOn(wrap.querySelector('img')!))).toBe(true);
    expect(shouldUseNativeMenu(eventOn(wrap.querySelector('a')!))).toBe(true);
    expect(shouldUseNativeMenu(eventOn(wrap.querySelector('span')!))).toBe(false);
  });

  it('data-native-menu opts a subtree out', () => {
    const wrap = document.createElement('div');
    wrap.setAttribute('data-native-menu', '');
    const child = document.createElement('em');
    wrap.appendChild(child);
    expect(shouldUseNativeMenu(eventOn(child))).toBe(true);
  });
});
