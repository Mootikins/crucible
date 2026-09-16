import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { WindowingProvider, useWindowing } from '@/windowing/components/context';

describe('WindowingProvider', () => {
  it('hands its slots and renderer to descendants', () => {
    const Probe = () => {
      const w = useWindowing();
      return (
        <div data-testid="probe">
          {w.renderContent(() => ({ id: 't', title: 'T', contentType: 'alpha' }))}
          {w.slots.corner?.()}
        </div>
      );
    };
    const { getByTestId } = render(() => (
      <WindowingProvider
        renderContent={(tab) => <span>{tab().contentType}</span>}
        slots={{ corner: () => <span>+corner</span> }}
      >
        <Probe />
      </WindowingProvider>
    ));
    expect(getByTestId('probe').textContent).toBe('alpha+corner');
  });

  it('throws outside a provider', () => {
    const Probe = () => {
      useWindowing();
      return null;
    };
    expect(() => render(() => <Probe />)).toThrow(/WindowingProvider/);
  });
});
