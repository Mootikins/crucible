import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { Drawer } from '@/components/mobile/Drawer';

function Harness(props: { onChange?: (open: boolean) => void; startOpen?: boolean }) {
  const [open, setOpen] = createSignal(props.startOpen ?? false);
  const change = (next: boolean) => {
    props.onChange?.(next);
    setOpen(next);
  };
  return (
    <>
      <button data-testid="opener" onClick={() => change(true)}>open</button>
      <Drawer side="left" label="Sessions" open={open()} onOpenChange={change} dragPx={null} width={300}>
        <button data-testid="first">first</button>
        <button data-testid="last">last</button>
      </Drawer>
    </>
  );
}

const panel = () => screen.getByTestId('drawer-left');

describe('Drawer', () => {
  it('is inert and hidden from assistive tech while closed', () => {
    render(() => <Harness />);
    expect(panel().hasAttribute('inert')).toBe(true);
    expect(panel().getAttribute('aria-hidden')).toBe('true');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('is a labelled modal dialog while open', () => {
    render(() => <Harness startOpen />);
    const dialog = screen.getByRole('dialog', { name: 'Sessions' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.hasAttribute('inert')).toBe(false);
  });

  it('closes on Escape', () => {
    const onChange = vi.fn();
    render(() => <Harness startOpen onChange={onChange} />);
    fireEvent.keyDown(panel(), { key: 'Escape' });
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it('closes on a scrim tap', () => {
    const onChange = vi.fn();
    render(() => <Harness startOpen onChange={onChange} />);
    fireEvent.click(screen.getByTestId('drawer-scrim-left'));
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  // A phone has no Escape key. The hardware back button is how a user
  // dismisses a drawer on Android, and it must not leave the app instead.
  it('closes on the hardware back button', () => {
    const onChange = vi.fn();
    render(() => <Harness onChange={onChange} />);
    fireEvent.click(screen.getByTestId('opener'));
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(onChange).toHaveBeenLastCalledWith(false);
  });

  it('moves focus into the drawer on open, and back to the opener on close', () => {
    render(() => <Harness />);
    const opener = screen.getByTestId('opener');
    opener.focus();
    fireEvent.click(opener);
    expect(panel().contains(document.activeElement)).toBe(true);
    fireEvent.keyDown(panel(), { key: 'Escape' });
    expect(document.activeElement).toBe(opener);
  });

  it('keeps Tab inside the open drawer', () => {
    render(() => <Harness startOpen />);
    const first = screen.getByTestId('first');
    const last = screen.getByTestId('last');
    last.focus();
    fireEvent.keyDown(panel(), { key: 'Tab' });
    expect(document.activeElement).toBe(first);
    first.focus();
    fireEvent.keyDown(panel(), { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(last);
  });
});
