/** The properties start folded to one line; a click opens them, a second click folds them. */
import { describe, expect, it } from 'vitest';
import { fireEvent, render } from '@solidjs/testing-library';
import { NoteProperties } from '../NoteProperties';

describe('NoteProperties', () => {
  it('starts folded, and each click toggles the fold', () => {
    const { getByRole, container } = render(() => (
      <NoteProperties props={{ title: 'Kilns', description: 'What makes a kiln', status: 'implemented', tags: ['kiln'] }} />
    ));
    const toggle = getByRole('button', { name: '3 properties' });
    const body = container.querySelector('.mk-propsbody')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(body.hasAttribute('inert')).toBe(true);
    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(body.hasAttribute('inert')).toBe(false);
    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
  });
});
