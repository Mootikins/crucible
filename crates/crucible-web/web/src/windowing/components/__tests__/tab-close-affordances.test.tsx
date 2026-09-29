import { describe, it, expect, beforeEach } from 'vitest';
import { fireEvent, render, waitFor } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { windowStore, windowActions } from '@/windowing/store';
import { findFirstPane } from '@/windowing/model/tree';
import { configureRails, neutralRenderer } from './fixtures';

/**
 * Every tab has a close control that a user can find. A theme can hide the
 * tab bars of a rail, so the rail icon carries its own close control. The tab
 * menu shows a bulk close only when it closes a tab. A tab also publishes its
 * content type, so a theme can style one kind of tab.
 */

const mount = () => render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);
const openMenu = () => document.querySelector<HTMLElement>('[role="menu"][data-state="open"]');
const menuLabels = () =>
  Array.from(openMenu()?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []).map((i) => i.textContent?.trim());

/** The rail icon of a tab, by its title. */
const railIcon = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>('.wm-ribbon-tab')).find((b) => b.title === title)!;

beforeEach(() => configureRails());

describe('the close control of a rail icon', () => {
  it('closes the tab of that icon', () => {
    mount();
    const close = railIcon('Omega').parentElement!.querySelector<HTMLButtonElement>('.wm-ribbon-tab-close')!;
    expect(close.getAttribute('aria-label')).toBe('Close Omega');
    fireEvent.click(close);
    const tabs = Object.values(windowStore.tabGroups).flatMap((g) => g.tabs.map((t) => t.id));
    expect(tabs).not.toContain('omega-tab');
  });

  it('is absent on a tab that the policy keeps', () => {
    configureRails({ mayCloseTab: (_s, _g, tabId) => tabId !== 'alpha-tab' });
    mount();
    expect(railIcon('Alpha').parentElement!.querySelector('.wm-ribbon-tab-close')).toBeNull();
    expect(railIcon('Omega').parentElement!.querySelector('.wm-ribbon-tab-close')).not.toBeNull();
  });
});

describe('the bulk closes of the tab menu', () => {
  it('hide when they would close nothing', async () => {
    mount();
    // Alpha is the only tab of its group: nothing is "other" or "to the right".
    fireEvent.contextMenu(railIcon('Alpha'));
    await waitFor(() => expect(menuLabels()).toContain('Close'));
    expect(menuLabels()).not.toContain('Close Others');
    expect(menuLabels()).not.toContain('Close to the Right');
  });

  it('show when they close a tab', async () => {
    mount();
    // Beta leads a group of three.
    fireEvent.contextMenu(railIcon('Beta'));
    await waitFor(() => expect(menuLabels()).toContain('Close Others'));
    expect(menuLabels()).toContain('Close to the Right');
  });
});

describe('the content type of a tab', () => {
  it('is published on the tab and on the rail icon', () => {
    const centre = findFirstPane(windowStore.layout)!.tabGroupId!;
    windowActions.addTab(centre, { id: 'doc-tab', title: 'Doc', contentType: 'note' });
    const { container } = mount();
    expect(container.querySelector<HTMLElement>('[data-tab-id="doc-tab"]')!.dataset.contentType).toBe('note');
    expect(railIcon('Alpha').dataset.contentType).toBe('alpha');
  });
});
