import { describe, it, expect, beforeEach } from 'vitest';
import { createComputed, onCleanup } from 'solid-js';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { FloatingWindow } from '../FloatingWindow';
import { windowStore, windowActions, setStore } from '@/windowing/store';
import { generateId } from '@/windowing/model/tree';
import type { WindowingContextValue } from '@/windowing/components/context';
import { CoreProviders, configureRails } from './fixtures';

describe('FloatingWindow — each panel reads its own tab', () => {
  let groupId: string;

  beforeEach(() => {
    configureRails();
    groupId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[groupId] = {
          id: groupId,
          tabs: [
            { id: 'first', title: 'First', contentType: 'alpha', metadata: { label: 'first-label' } },
            { id: 'second', title: 'Second', contentType: 'alpha', metadata: { label: 'second-label' } },
          ],
          activeTabId: 'first',
        };
      }),
    );
    windowActions.createFloatingWindow(groupId, 10, 10, 300, 200);
  });

  /**
   * The cleanup of the old panel runs after the store names the tab that the
   * user selected. Its accessor must still give the old tab.
   */
  it('never gives the first panel the metadata of the second tab', () => {
    const seen: Record<string, unknown[]> = {};
    const recorder: WindowingContextValue['renderContent'] = (tab) => {
      const id = tab().id;
      seen[id] = [];
      createComputed(() => seen[id].push(tab().metadata?.label));
      onCleanup(() => seen[id].push(tab().metadata?.label));
      return <div data-testid={`body-${id}`} />;
    };
    const { queryByTestId } = render(() => (
      <CoreProviders renderContent={recorder}>
        <FloatingWindow window={windowStore.floatingWindows[0]} />
      </CoreProviders>
    ));
    expect(queryByTestId('body-first')).toBeTruthy();

    windowActions.setActiveTab(groupId, 'second');

    expect(queryByTestId('body-second')).toBeTruthy();
    expect(seen.first).not.toContain('second-label');
    expect(seen.second).toEqual(['second-label']);
  });
});
