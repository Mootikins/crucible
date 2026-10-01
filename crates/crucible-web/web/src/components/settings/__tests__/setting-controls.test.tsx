import { createSignal } from 'solid-js';
import { fireEvent, render, screen } from '@solidjs/testing-library';
import { describe, expect, it, vi } from 'vitest';
import { BooleanSetting, RangeSetting } from '../primitives';

describe('shared setting controls', () => {
  it('emits numeric range input and renders the updated value', () => {
    const update = vi.fn();
    render(() => {
      const [value, setValue] = createSignal(8);
      return <table><tbody><RangeSetting label="Contrast" min={0} max={16} value={value()}
        onInput={(next) => { update(next); setValue(next); }} /></tbody></table>;
    });
    fireEvent.input(screen.getByLabelText('Contrast'), { target: { value: '12' } });
    expect(update).toHaveBeenCalledWith(12);
    expect(screen.getByRole('status')).toHaveTextContent('12');
  });

  it('emits checkbox state through its accessible setting label', () => {
    const update = vi.fn();
    render(() => <table><tbody><BooleanSetting label="True black" checked={false} onChange={update} /></tbody></table>);
    fireEvent.click(screen.getByLabelText('True black'));
    expect(update).toHaveBeenCalledWith(true);
  });
});
