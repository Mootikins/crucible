import { describe, it, expect, beforeEach } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { DragDropProvider } from '@thisbeyond/solid-dnd';
import { configureWindowing, windowStore } from '@/windowing/store';
import { WindowingProvider } from '@/windowing/components/context';
import { stubPolicy } from '@/windowing/__tests__/stubPolicy';
import { primaryEdgeGroupId } from '@/windowing/model/tree';
import { Ribbon } from '../Ribbon';

/** A tab that the policy gives a reason is greyed out, and says why. */
beforeEach(() => {
  configureWindowing(
    stubPolicy({
      seed: () => {
        const s = stubPolicy().seed();
        const g = primaryEdgeGroupId(s, 'right')!;
        s.tabGroups[g]!.tabs = [
          { id: 'ok', title: 'Fine', contentType: 'alpha' },
          { id: 'no', title: 'Remote', contentType: 'beta' },
        ];
        s.tabGroups[g]!.activeTabId = 'ok';
        return s;
      },
      unavailableReason: (tab) => (tab.contentType === 'beta' ? 'needs the host' : null),
    }),
  );
});

const buttons = (container: HTMLElement) =>
  Array.from(container.querySelectorAll<HTMLButtonElement>('[data-testid="collapsed-tab-button-right"]'));

describe('Ribbon — an unavailable tab', () => {
  it('shows the policy reason in its tooltip, and greys out', () => {
    const { container } = render(() => (
      <WindowingProvider renderContent={() => null} slots={{}}>
        <DragDropProvider>
          <Ribbon position="right" />
        </DragDropProvider>
      </WindowingProvider>
    ));
    const [ok, no] = buttons(container);
    expect(ok!.getAttribute('title')).toBe('Fine');
    expect(no!.getAttribute('title')).toBe('Remote — needs the host');
    expect(no!.className).toContain('cursor-not-allowed');

    // A click does nothing: the rail stays shut.
    fireEvent.click(no!);
    expect(windowStore.edgePanels.right.mode).toBe('strip');
  });
});
