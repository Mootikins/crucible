import { Component } from 'solid-js';

/**
 * The disclosure caret on a chip trigger.
 *
 * Authored rather than borrowed. It was `lucide`'s `ChevronDown` at 12px,
 * which is a NAVIGATION chevron: 12 of its 24 viewBox units wide, so it landed
 * 6px wide against a 10px cap height — a broad, shallow V almost as wide as
 * the letters beside it. `stroke-linecap: round` then put a half-pixel bulb on
 * each of its three points, which at a 1px stroke is most of the mark, so it
 * read soft and smudged rather than crisp.
 *
 * This one is drawn for the job: 5px wide, 2.5px tall, mitred and square-cut
 * so every pixel of it is stroke. It is half the width of what it replaces and
 * reads as punctuation after the label rather than as a second glyph.
 *
 * Colour comes from `currentColor`, so the trigger's own hover and open states
 * carry it — the caret must never be a separately-tinted thing.
 */
export const Caret: Component<{ class?: string; classList?: Record<string, boolean | undefined> }> = (props) => (
  <svg
    viewBox="0 0 10 10"
    width="10"
    height="10"
    fill="none"
    aria-hidden="true"
    class={props.class}
    classList={props.classList}
  >
    <path
      d="M2.5 4 5 6.5 7.5 4"
      stroke="currentColor"
      stroke-width="1.1"
      stroke-linecap="square"
      stroke-linejoin="miter"
    />
  </svg>
);
