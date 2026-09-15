import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { IconButton } from '../IconButton';

/**
 * The chrome icon button keeps a 28px visual box (24px in dense strips) and
 * takes clicks over a 32px box. The extra area comes from the `hit-32`
 * pseudo-element in styles/refine-touch.css, which needs a positioned
 * ancestor — the button itself. A test on the classes is the only check
 * available here: jsdom computes no layout, so it cannot measure the box.
 */
describe('IconButton', () => {
  const button = (el: HTMLElement) => el.querySelector('button')!;

  it('keeps the 28px visual box at the default size', () => {
    const { container } = render(() => <IconButton aria-label="Fit" />);
    expect(button(container).classList.contains('w-7')).toBe(true);
    expect(button(container).classList.contains('h-7')).toBe(true);
  });

  it('keeps the 24px visual box at the small size', () => {
    const { container } = render(() => <IconButton size="sm" aria-label="Fit" />);
    expect(button(container).classList.contains('w-6')).toBe(true);
  });

  it('carries the larger hit area and the position it needs', () => {
    const { container } = render(() => <IconButton aria-label="Fit" />);
    expect(button(container).classList.contains('hit-32')).toBe(true);
    expect(button(container).classList.contains('relative')).toBe(true);
  });

  /** One focus treatment for the whole app: the `focus-ring` utility, never a
   * bare `outline-none`. A gate in style-consistency.test.ts checks the tree. */
  it('uses the shared focus ring', () => {
    const { container } = render(() => <IconButton aria-label="Fit" />);
    expect(button(container).classList.contains('focus-ring')).toBe(true);
    expect([...button(container).classList].some((c) => c.endsWith('outline-none'))).toBe(false);
  });

  it('keeps a caller class', () => {
    const { container } = render(() => <IconButton aria-label="Fit" class="ml-1" />);
    expect(button(container).classList.contains('ml-1')).toBe(true);
    expect(button(container).classList.contains('hit-32')).toBe(true);
  });
});
