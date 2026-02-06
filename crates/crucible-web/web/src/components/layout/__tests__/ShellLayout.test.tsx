import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
import { ShellLayout } from '../ShellLayout';

vi.mock('@/components/BreadcrumbNav', () => ({
  BreadcrumbNav: () => <div data-testid="breadcrumb-nav" />,
}));

describe('ShellLayout - drawer animation', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it('loads zone widths from localStorage', () => {
    localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 300, right: 400, bottom: 250 }));
    const { getByTestId } = render(() => <ShellLayout />);
    const leftZone = getByTestId('zone-left');
    expect(leftZone.style.flexBasis).toBe('300px');
  });

  it('uses default widths when localStorage is empty', () => {
    const { getByTestId } = render(() => <ShellLayout />);
    const leftZone = getByTestId('zone-left');
    const rightZone = getByTestId('zone-right');
    const bottomZone = getByTestId('zone-bottom');
    expect(leftZone.style.flexBasis).toBe('280px');
    expect(rightZone.style.flexBasis).toBe('350px');
    expect(bottomZone.style.flexBasis).toBe('0px');
  });

  it('collapses zone to flex-basis 0px on toggle', async () => {
    const { getByTestId } = render(() => <ShellLayout />);
    const toggleLeft = getByTestId('toggle-left');
    const leftZone = getByTestId('zone-left');

    expect(leftZone.style.flexBasis).toBe('280px');
    await fireEvent.click(toggleLeft);
    expect(leftZone.style.flexBasis).toBe('0px');
  });

  it('restores zone to saved width on toggle back', async () => {
    localStorage.setItem('crucible:zone-widths', JSON.stringify({ left: 300, right: 400, bottom: 250 }));
    localStorage.setItem('crucible:zones', JSON.stringify({ left: 'hidden', right: 'visible', bottom: 'hidden' }));
    const { getByTestId } = render(() => <ShellLayout />);
    const toggleLeft = getByTestId('toggle-left');
    const leftZone = getByTestId('zone-left');

    expect(leftZone.style.flexBasis).toBe('0px');
    await fireEvent.click(toggleLeft);
    expect(leftZone.style.flexBasis).toBe('300px');
  });

  it('sets opacity 0 on collapsed sidebar', async () => {
    const { getByTestId } = render(() => <ShellLayout />);
    const toggleLeft = getByTestId('toggle-left');
    const leftZone = getByTestId('zone-left');

    expect(leftZone.style.opacity).toBe('1');
    await fireEvent.click(toggleLeft);
    expect(leftZone.style.opacity).toBe('0');
  });

  it('calls onZoneTransitionEnd on flex-basis transitionend', () => {
    const handler = vi.fn();
    const { getByTestId } = render(() => <ShellLayout onZoneTransitionEnd={handler} />);
    const leftZone = getByTestId('zone-left');

    const event = new Event('transitionend', { bubbles: true });
    Object.defineProperty(event, 'propertyName', { value: 'flex-basis' });
    leftZone.dispatchEvent(event);

    expect(handler).toHaveBeenCalledWith('left');
  });

  it('does not call onZoneTransitionEnd for non-flex-basis transitions', () => {
    const handler = vi.fn();
    const { getByTestId } = render(() => <ShellLayout onZoneTransitionEnd={handler} />);
    const leftZone = getByTestId('zone-left');

    const event = new Event('transitionend', { bubbles: true });
    Object.defineProperty(event, 'propertyName', { value: 'opacity' });
    leftZone.dispatchEvent(event);

    expect(handler).not.toHaveBeenCalled();
  });

  it('does not save zone widths on toggle (widths only change via resize)', async () => {
    const { getByTestId } = render(() => <ShellLayout />);
    const toggleLeft = getByTestId('toggle-left');
    await fireEvent.click(toggleLeft);

    const stored = localStorage.getItem('crucible:zone-widths');
    expect(stored).toBeNull();
  });

  it('handles rapid toggle without stuck states', async () => {
    const { getByTestId } = render(() => <ShellLayout />);
    const toggleLeft = getByTestId('toggle-left');
    const leftZone = getByTestId('zone-left');

    for (let i = 0; i < 5; i++) {
      await fireEvent.click(toggleLeft);
    }

    expect(leftZone.style.flexBasis).toBe('0px');
    expect(leftZone.style.opacity).toBe('0');

    await fireEvent.click(toggleLeft);
    expect(leftZone.style.flexBasis).toBe('280px');
    expect(leftZone.style.opacity).toBe('1');
  });
});
