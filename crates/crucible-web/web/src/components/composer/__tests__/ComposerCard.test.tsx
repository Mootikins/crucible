import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { readFileSync } from 'fs';
import { resolve } from 'path';

/**
 * The composer's shape, and only its shape.
 *
 * Two facts are under test, and both were defects a user could see:
 *   1. The pickers sat INSIDE the prompt, so the field carried five labels
 *      that had nothing to do with the message being written.
 *   2. The prompt was a 9999px pill that squared off to 10px the moment the
 *      text wrapped, so the control changed shape while you typed in it.
 */

vi.mock('@/hooks/useMediaRecorder', () => ({
  useMediaRecorder: () => ({
    isRecording: () => false,
    audioLevel: () => 0,
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
  }),
}));

vi.mock('@/hooks/useAutocomplete', () => ({
  useAutocomplete: () => ({
    isOpen: () => false,
    items: () => [],
    selectedIndex: () => -1,
    onInput: vi.fn(),
    onKeyDown: vi.fn(),
    complete: vi.fn(),
    close: vi.fn(),
  }),
}));

vi.mock('@/components/MicButton', () => ({
  MicButton: () => <button type="button" data-testid="mic-button" />,
}));

const { ComposerCard } = await import('../ComposerCard');

const [value, setValue] = createSignal('');

afterEach(() => {
  cleanup();
  setValue('');
});

const renderCard = (props: { docked?: boolean } = {}) =>
  render(() => (
    <ComposerCard
      value={value}
      setValue={setValue}
      placeholder="Type a message..."
      testid="prompt"
      onSubmit={vi.fn()}
      docked={props.docked}
      chips={<button type="button" data-testid="model-chip" />}
      trailing={<div data-testid="scope-chips" />}
      action={<button type="button" data-testid="send" />}
    />
  ));

/** The prompt's own element — the bordered surface, not the textarea. */
const surface = () => document.querySelector('.composer-surface') as HTMLElement;

describe('ComposerCard — the prompt holds the message, nothing else', () => {
  it('keeps the textarea and the commit button inside the surface', () => {
    renderCard();
    expect(surface().contains(screen.getByTestId('prompt'))).toBe(true);
    expect(surface().contains(screen.getByTestId('send'))).toBe(true);
  });

  it('keeps the mic inside the surface: it is an input control, not a fact', () => {
    renderCard();
    expect(surface().contains(screen.getByTestId('mic-button'))).toBe(true);
  });

  it('puts the pickers and the scope chips OUTSIDE it', () => {
    renderCard();
    for (const id of ['model-chip', 'scope-chips']) {
      expect(surface().contains(screen.getByTestId(id))).toBe(false);
    }
  });

  it('draws them on one row below, pickers first and the quietest last', () => {
    renderCard();
    const row = screen.getByTestId('composer-controls');
    const order = ['model-chip', 'scope-chips'].map((id) =>
      Array.from(row.querySelectorAll('[data-testid]')).indexOf(screen.getByTestId(id)),
    );
    expect(order.every((i) => i >= 0)).toBe(true);
    expect(order).toEqual([...order].sort((a, b) => a - b));
  });
});

describe('ComposerCard — one shape at every height', () => {
  it('states no multi-line mode for the stylesheet to react to', () => {
    renderCard();
    // The morph was driven by this attribute. Nothing sets it any more, and
    // the radius does not depend on it.
    expect(surface().getAttribute('data-multiline')).toBeNull();
  });

  it('carries one padding, not a pair that swaps with the line count', () => {
    renderCard();
    const classes = Array.from(surface().classList);
    expect(classes).toContain('px-3.5');
    expect(classes).not.toContain('px-4');
    expect(classes).not.toContain('px-2.5');
  });

  it('sets the radius from the composer token and never from a pill', () => {
    // The radius lives in CSS, so the gate reads the CSS. A class assertion
    // here would pass while the stylesheet still drew a 9999px pill.
    const file = readFileSync(
      resolve(__dirname, '../../../styles/refine-composer.css'),
      'utf-8',
    );
    // Declarations only. The prose above them names the pill it replaced.
    const css = file.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(css).toMatch(/\.composer-surface\s*\{[^}]*border-radius:\s*var\(--radius-composer\)/);
    expect(css).not.toContain('9999px');
  });
});

describe('ComposerCard — docked', () => {
  it('squares its top edge only while a card sits on it', () => {
    renderCard({ docked: true });
    expect(surface().getAttribute('data-docked')).toBe('true');
    cleanup();
    renderCard({ docked: false });
    expect(surface().getAttribute('data-docked')).toBeNull();
  });
});
