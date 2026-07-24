import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { iconForAgent } from '../agent-icons';
import { Bot } from '@/lib/icons';

const renders = (name: string) => {
  const Icon = iconForAgent(name);
  const { container } = render(() => <Icon class="w-4 h-4" />);
  return container.querySelector('svg')!;
};

describe('iconForAgent', () => {
  it('gives every built-in ACP agent its own distinct mark', () => {
    const builtins = ['opencode', 'claude', 'gemini', 'codex', 'cursor'];
    const marks = builtins.map((n) => iconForAgent(n));
    expect(new Set(marks).size).toBe(builtins.length);
    // …and none of them is the generic fallback.
    expect(marks).not.toContain(Bot);
  });

  it('is case- and whitespace-insensitive', () => {
    expect(iconForAgent('  Claude ')).toBe(iconForAgent('claude'));
  });

  it('keeps the family mark for a custom profile that extends a built-in', () => {
    expect(iconForAgent('my-claude')).toBe(iconForAgent('claude'));
    expect(iconForAgent('opencode-staging')).toBe(iconForAgent('opencode'));
  });

  it('falls back to the generic robot for unknown or empty names', () => {
    expect(iconForAgent('some-vendor')).toBe(Bot);
    expect(iconForAgent('')).toBe(Bot);
  });

  // Regression: the marks were module-level JSX, i.e. ONE DOM node each.
  // Rendering the same mark twice (claude + my-claude in one picker) MOVED
  // the node into the second slot and left the first row empty.
  it('renders independently when the same mark is used twice', () => {
    const A = iconForAgent('claude');
    const B = iconForAgent('my-claude');
    const { container } = render(() => (
      <>
        <A class="a" />
        <B class="b" />
      </>
    ));
    const first = container.querySelector('svg.a');
    const second = container.querySelector('svg.b');
    expect(first?.childElementCount).toBeGreaterThan(0);
    expect(second?.childElementCount).toBe(first?.childElementCount);
  });

  it('renders an inline currentColor svg (no external asset, no brand colour)', () => {
    const svg = renders('gemini');
    expect(svg.getAttribute('class')).toBe('w-4 h-4');
    expect(svg.getAttribute('aria-hidden')).toBe('true');
    // Every stroke/fill resolves to currentColor so the mark themes with text.
    expect(svg.outerHTML).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    expect(svg.outerHTML).not.toMatch(/<image|xlink:href|url\(/i);
  });
});
