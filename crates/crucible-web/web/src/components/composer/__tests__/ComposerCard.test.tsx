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
 *      that had nothing to do with the message being written. They now sit
 *      on ONE data-built row (`ChipRow`) under the capsule, on both surfaces.
 *   2. The prompt was an 18px box at every height, which never read as a
 *      pill. It is a stadium at one line and a card once the text wraps,
 *      and the switch is one attribute the stylesheet reads.
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
      chips={[
        {
          key: 'model',
          label: 'Model',
          value: 'glm',
          options: [{ value: 'glm', label: 'glm' }],
          onSelect: vi.fn(),
          testid: 'model-chip',
        },
        { key: 'project', label: 'Project', value: 'crucible', render: 'static', testid: 'scope-chips' },
      ]}
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

  it('puts every chip OUTSIDE it: a chip is never a descendant of the capsule', () => {
    renderCard();
    for (const id of ['model-chip', 'scope-chips']) {
      expect(surface().contains(screen.getByTestId(id))).toBe(false);
    }
  });

  it('draws the chips on the shared row BELOW the capsule, in list order', () => {
    renderCard();
    const row = screen.getByTestId('composer-chip-row');
    expect(
      surface().compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const order = ['model-chip', 'scope-chips'].map((id) =>
      Array.from(row.querySelectorAll('[data-testid]')).indexOf(screen.getByTestId(id)),
    );
    expect(order.every((i) => i >= 0)).toBe(true);
    expect(order).toEqual([...order].sort((a, b) => a - b));
  });

  it('draws no row when the list is empty', () => {
    render(() => (
      <ComposerCard
        value={value}
        setValue={setValue}
        placeholder="Type a message..."
        testid="prompt"
        onSubmit={vi.fn()}
        action={<button type="button" data-testid="send" />}
      />
    ));
    expect(screen.queryByTestId('composer-chip-row')).toBeNull();
  });
});

describe('ComposerCard — a pill at one line, a card at many', () => {
  it('states one line at rest and many once the value gains a second line', () => {
    renderCard();
    expect(surface().getAttribute('data-lines')).toBe('one');
    setValue('first line');
    expect(surface().getAttribute('data-lines')).toBe('one');
    setValue('first line\nsecond line');
    expect(surface().getAttribute('data-lines')).toBe('many');
    // A send clears the prompt; the pill comes back with it.
    setValue('');
    expect(surface().getAttribute('data-lines')).toBe('one');
  });

  it('states no multi-line class of its own: the attribute is the only switch', () => {
    renderCard();
    setValue('a\nb');
    const classes = Array.from(surface().classList);
    expect(classes.some((c) => /rounded/.test(c))).toBe(false);
  });

  it('carries one padding, not a pair that swaps with the line count', () => {
    renderCard();
    const classes = Array.from(surface().classList);
    expect(classes).toContain('px-3.5');
    expect(classes).not.toContain('px-4');
    expect(classes).not.toContain('px-2.5');
  });

  it('draws a 9999px stadium for one line and the composer token for many', () => {
    // The radius lives in CSS, so the gate reads the CSS. A class assertion
    // here would pass while the stylesheet still drew one shape for both.
    const file = readFileSync(
      resolve(__dirname, '../../../styles/refine-composer.css'),
      'utf-8',
    );
    // Declarations only. The prose above them names the shapes.
    const css = file.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(css).toMatch(/\.composer-surface\s*\{[^}]*border-radius:\s*var\(--radius-composer\)/);
    expect(css).toMatch(/\.composer-surface\[data-lines='one'\]\s*\{[^}]*border-radius:\s*9999px/);
    // The docked rule comes AFTER the stadium rule at equal specificity, so
    // a docked one-line prompt keeps its flat top edge.
    expect(css.indexOf("[data-lines='one']")).toBeLessThan(css.indexOf("[data-docked='true']"));
    // The placeholder must not wrap: a wrapped hint counts in scrollHeight
    // and would grow the EMPTY field past one line.
    expect(css).toMatch(/textarea::placeholder\s*\{[^}]*white-space:\s*nowrap/);
    // The token itself is the card radius, not a pill.
    const theme = readFileSync(resolve(__dirname, '../../../index.css'), 'utf-8');
    expect(theme).toMatch(/--cru-radius-composer:\s*18px/);
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
