/**
 * The rules that the mockup's primitives promise to a port: an icon button
 * always has a name, and a reject that reverts on disk asks once first.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render } from '@solidjs/testing-library';
import { DecisionButtons } from '../DecisionButtons';
import { IconButton } from '../IconButton';

describe('IconButton', () => {
  it('names the button with its label, as a tooltip by default', () => {
    const { getByRole } = render(() => <IconButton label="Close"><svg /></IconButton>);
    const button = getByRole('button', { name: 'Close' });
    expect(button.getAttribute('title')).toBe('Close');
  });

  it('names the button without a tooltip when the label is for assistive technology only', () => {
    const { getByRole } = render(() => (
      <IconButton label="Remove" labelAs="aria">
        <svg />
      </IconButton>
    ));
    const button = getByRole('button', { name: 'Remove' });
    expect(button.hasAttribute('title')).toBe(false);
  });

  it('does not compile without a label', () => {
    // @ts-expect-error: an icon-only button has no other name, so `label` is required.
    const unnamed = () => <IconButton><svg /></IconButton>;
    expect(typeof unnamed).toBe('function');
  });
});

describe('DecisionButtons', () => {
  it('rejects only on the second click when the reject asks for confirmation', () => {
    const onReject = vi.fn();
    const { getByRole } = render(() => <DecisionButtons onAccept={() => {}} onReject={onReject} confirmReject="Revert on disk?" />);
    fireEvent.click(getByRole('button', { name: 'Reject' }));
    expect(onReject).not.toHaveBeenCalled();
    fireEvent.click(getByRole('button', { name: 'Revert on disk?' }));
    expect(onReject).toHaveBeenCalledTimes(1);
  });

  it('rejects on the first click without a confirmation', () => {
    const onReject = vi.fn();
    const { getByRole } = render(() => <DecisionButtons onAccept={() => {}} onReject={onReject} />);
    fireEvent.click(getByRole('button', { name: 'Reject' }));
    expect(onReject).toHaveBeenCalledTimes(1);
  });
});
