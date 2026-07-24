import type { Component, JSX } from 'solid-js';
import { Bot } from '@/lib/icons';

/**
 * Marks for the ACP agents Crucible can drive (opencode, claude, gemini,
 * codex, cursor — see BUILTIN_PROFILES in crucible-daemon/src/acp/discovery).
 *
 * These are simple monochrome glyphs drawn to EVOKE each vendor's mark, not
 * copies of their brand assets: they inherit currentColor so they sit in the
 * chip row like every other icon, and they carry no wordmark or brand colour.
 * A custom profile that extends a built-in (`my-claude`) resolves by name, and
 * anything unrecognised falls back to the generic robot.
 */

type IconProps = { class?: string };

/**
 * `body` is a FUNCTION, not JSX. Solid's JSX creates real DOM nodes eagerly,
 * so a module-level element would be ONE node shared by every render — the
 * second mount moves it out of the first, leaving the earlier row blank
 * wherever a mark appears twice in a list (`claude` and `my-claude`).
 */
const svg = (body: () => JSX.Element, opts: { fill?: boolean } = {}): Component<IconProps> => {
  return (props: IconProps) => (
    <svg
      class={props.class}
      viewBox="0 0 24 24"
      fill={opts.fill ? 'currentColor' : 'none'}
      stroke={opts.fill ? 'none' : 'currentColor'}
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {body()}
    </svg>
  );
};

/**
 * Radial burst of tapered spokes. Filled wedges rather than strokes: at the
 * 14px chip size a thin-stroked asterisk washes out to nothing.
 */
const ClaudeMark = svg(
  () => (
    <>
      <path d="M12 12 10.7 3.2 13.3 3.2Z" />
      <path d="M12 12 10.7 3.2 13.3 3.2Z" transform="rotate(60 12 12)" />
      <path d="M12 12 10.7 3.2 13.3 3.2Z" transform="rotate(120 12 12)" />
      <path d="M12 12 10.7 3.2 13.3 3.2Z" transform="rotate(180 12 12)" />
      <path d="M12 12 10.7 3.2 13.3 3.2Z" transform="rotate(240 12 12)" />
      <path d="M12 12 10.7 3.2 13.3 3.2Z" transform="rotate(300 12 12)" />
    </>
  ),
  { fill: true },
);

/** Four-pointed sparkle. */
const GeminiMark = svg(() => (
  <path d="M12 2.5c0 5.25 3.75 9 9 9.5-5.25.5-9 4.25-9 9.5 0-5.25-3.75-9-9-9.5 5.25-.5 9-4.25 9-9.5Z" />
));

/** Interlocking rosette — the knot-shaped mark's silhouette. */
const CodexMark = svg(() => (
  <>
    <ellipse cx="12" cy="12" rx="3.6" ry="8.5" />
    <ellipse cx="12" cy="12" rx="3.6" ry="8.5" transform="rotate(60 12 12)" />
    <ellipse cx="12" cy="12" rx="3.6" ry="8.5" transform="rotate(120 12 12)" />
  </>
));

/** Isometric cube. */
const CursorMark = svg(() => (
  <>
    <path d="M12 2.75 20.5 7.5v9L12 21.25 3.5 16.5v-9L12 2.75Z" />
    <path d="M12 12 20.5 7.5M12 12v9.25M12 12 3.5 7.5" />
  </>
));

/** Terminal prompt. */
const OpenCodeMark = svg(() => (
  <>
    <rect x="3" y="4.5" width="18" height="15" rx="2.5" />
    <path d="M7.5 9.5 10.5 12l-3 2.5M13 15h3.5" />
  </>
));

// Longest key first: a custom name is matched by substring, so a shorter key
// must never win over a longer one that also matches.
const AGENT_MARKS: [string, Component<IconProps>][] = [
  ['opencode', OpenCodeMark],
  ['claude', ClaudeMark],
  ['gemini', GeminiMark],
  ['cursor', CursorMark],
  ['codex', CodexMark],
];

/**
 * The mark for an ACP agent profile name. Exact match first, then substring so
 * custom profiles (`my-claude`, `claude-staging`) keep their family's mark.
 * Returns the generic robot for anything unknown — never undefined, so every
 * row in the picker is iconed.
 */
export function iconForAgent(name: string): Component<IconProps> {
  const key = name.trim().toLowerCase();
  if (!key) return Bot;
  for (const [agent, mark] of AGENT_MARKS) if (key === agent) return mark;
  for (const [agent, mark] of AGENT_MARKS) if (key.includes(agent)) return mark;
  return Bot;
}
