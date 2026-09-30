import { describe, it, expect, beforeEach } from 'vitest';
import { fireEvent, render } from '@solidjs/testing-library';
import { WindowManager } from '../WindowManager';
import { windowActions, windowStore } from '@/windowing/store';
import { collectLeafGroupIds, findFirstPane } from '@/windowing/model/tree';
import { configureRails, neutralRenderer } from './fixtures';

/** The pop-out button of a tab bar moves the active tab, not its neighbours. */

const mount = () => render(() => <WindowManager renderContent={neutralRenderer} slots={{}} />);
const button = () => document.querySelector<HTMLButtonElement>('button[title="Pop out to floating window"]');
const centreTabs = () =>
  collectLeafGroupIds(windowStore.layout).flatMap((id) => windowStore.tabGroups[id]!.tabs.map((t) => t.id));

/** Two tabs in the centre, the first one active. */
function fillCentre() {
  const centre = findFirstPane(windowStore.layout)!.tabGroupId!;
  windowActions.addTab(centre, { id: 'doc-one', title: 'One', contentType: 'note' });
  windowActions.addTab(centre, { id: 'doc-two', title: 'Two', contentType: 'note' });
  windowActions.setActiveTab(centre, 'doc-one');
}

describe('the pop-out button', () => {
  beforeEach(() => {
    configureRails();
    fillCentre();
  });

  it('moves the active tab only, and its neighbours stay', () => {
    mount();
    fireEvent.click(button()!);
    const [win] = windowStore.floatingWindows;
    expect(windowStore.tabGroups[win!.tabGroupId]!.tabs.map((t) => t.id)).toEqual(['doc-one']);
    expect(centreTabs()).toEqual(['doc-two']);
  });

  it('hides on a tab that the policy keeps', () => {
    configureRails({ mayCloseTab: (_s, _g, tabId) => tabId !== 'doc-one' });
    fillCentre();
    mount();
    expect(button()).toBeNull();
  });
});
