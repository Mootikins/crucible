import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { Ribbon } from '../Ribbon';
import { CoreProviders, configureRails } from './fixtures';

const swapTitle = () => {
  const { getByTestId, unmount } = render(() => (
    <CoreProviders>
      <Ribbon position="left" />
    </CoreProviders>
  ));
  const title = getByTestId('ribbon-cmd-swap-sides').getAttribute('title');
  unmount();
  return title;
};

describe('Ribbon — the swap button names the chord from the policy', () => {
  it('prints the chord that the policy binds', () => {
    configureRails({
      shortcuts: [{ key: 's', modifiers: ['alt'], action: 'swapSidePanels', description: 'Swap' }],
    });
    expect(swapTitle()).toBe('Swap side panels (Alt+S)');
  });

  it('prints no chord when the policy binds none', () => {
    configureRails({ shortcuts: [] });
    expect(swapTitle()).toBe('Swap side panels');
  });
});
