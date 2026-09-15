import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'fs';
import { resolve, join } from 'path';
import { contractDark, contractLight, resolveToken } from '@/test-utils/css-tokens';

/**
 * Design-system contract: the 2026-07-17 styling pass migrated every raw
 * Tailwind gray/status palette class to the semantic tokens in index.css
 * and gave structural surfaces mount animations. These tests keep both
 * from regressing — new code must use tokens, not raw palettes.
 */

const SRC = resolve(__dirname, '../..');

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    // test-harness/ is test scaffolding, not shipped UI.
    if (entry === 'node_modules' || entry === 'test-harness' || entry.startsWith('.')) continue;
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(tsx|ts)$/.test(entry) && !/\.test\.tsx?$/.test(entry)) out.push(p);
  }
  return out;
}

const read = (rel: string) => readFileSync(resolve(SRC, rel), 'utf-8');

describe('semantic color tokens (no raw palettes)', () => {
  // Allowlist, not a denylist: ANY Tailwind palette color-NNN is an offender
  // unless its hue is approved. A denylist that grows one hue at a time gives a
  // false green — whole families (orange, blue, indigo, teal, …) slipped through.
  // sky-* is deliberately allowed: it marks spawned/running agents (the theme
  // has no blue token yet).
  const ALL_TW_COLORS = [
    'slate', 'gray', 'zinc', 'neutral', 'stone', 'red', 'orange', 'amber',
    'yellow', 'lime', 'green', 'emerald', 'teal', 'cyan', 'sky', 'blue',
    'indigo', 'violet', 'purple', 'fuchsia', 'pink', 'rose',
  ];
  const APPROVED_HUES = new Set([
    'neutral', 'zinc', 'gray', 'slate', 'stone', 'emerald', 'green', 'amber',
    'yellow', 'red', 'purple', 'violet', 'sky',
  ]);
  const PALETTE_RE = new RegExp(`\\b(${ALL_TW_COLORS.join('|')})-[0-9]{2,3}\\b`, 'g');

  it('no component uses an off-token Tailwind palette class', () => {
    const offenders: string[] = [];
    // index.html is outside src/ but carries classes too (the body classes
    // hid a neutral-950 canvas behind the shell for months).
    for (const file of [...walk(SRC), resolve(SRC, '../index.html')]) {
      const lines = readFileSync(file, 'utf-8').split('\n');
      lines.forEach((line, i) => {
        for (const m of line.matchAll(PALETTE_RE)) {
          if (!APPROVED_HUES.has(m[1])) {
            offenders.push(`${file}:${i + 1}: ${line.trim()}`);
            break;
          }
        }
      });
    }
    expect(offenders).toEqual([]);
  });

  it('surfaces use tokens, not white-alpha (bg/text/border)', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const src = readFileSync(file, 'utf-8');
      if (/\b(?:bg|text|border)-white\//.test(src)) offenders.push(file);
    }
    expect(offenders).toEqual([]);
  });
});

describe('motion primitives on structural surfaces', () => {
  it('floating windows pop in', () => {
    expect(read('components/windowing/FloatingWindow.tsx')).toMatch(/cru-anim-pop/);
  });

  it('edge panels slide via one rAF-driven progress (frame + translate locked)', () => {
    const src = read('components/windowing/EdgePanel.tsx');
    // A single progress value drives the clip frame size AND the inner
    // translate each frame, so neighbors reflow smoothly over the whole
    // toggle and the clip edge never tears from the panel edge. Content
    // stays MOUNTED while collapsed (an expand must never pay a
    // panel-subtree mount) and leaves paint/tab order via visibility.
    expect(src).toMatch(/TWEEN_MS/);
    expect(src).toMatch(/requestAnimationFrame\(step\)/);
    expect(src).toMatch(/visibility: progress\(\)/);
    expect(src).not.toMatch(/setRendered/);
    // NO CSS transitions on the slide: width/height transition on the main
    // thread while translate composites — under load they desync and tear.
    expect(src).not.toMatch(/transition:.*(width|height|translate)/);
  });

  it('command palette pops in over a fading overlay', () => {
    const src = read('components/CommandPalette.tsx');
    expect(src).toMatch(/cru-anim-pop/);
    expect(src).toMatch(/cru-anim-fade/);
  });

  it('autocomplete and hover-preview cards rise in', () => {
    expect(read('components/AutocompletePopup.tsx')).toMatch(/cru-anim-rise/);
    expect(read('components/WikilinkHoverPreview.tsx')).toMatch(/cru-anim-rise/);
  });

  it('keyframes animate scale/translate, never transform (would clobber Tailwind transforms)', () => {
    const css = read('index.css');
    // pop, rise, fade — edge panels tween width/height inline (no keyframes).
    const keyframeBlocks = css.match(/@keyframes cru-[\s\S]*?\n\}/g) ?? [];
    expect(keyframeBlocks.length).toBeGreaterThanOrEqual(3);
    for (const block of keyframeBlocks) {
      expect(block).not.toMatch(/transform:/);
    }
  });
});

describe('one focus treatment (R6)', () => {
  /**
   * `focus-ring` is the ONE focus utility (index.css). A line that writes
   * `outline-none` or `focus:outline-none` without it removes the only
   * affordance a keyboard user has, or builds a second ring by hand.
   *
   * ONE exception, and index.css documents it: a control whose focus the
   * CONTAINER draws. The composer textarea sits in a card that takes
   * `focus-within:border-primary`, so its own offset ring would put two
   * ember treatments 2px apart on one control.
   */
  const CONTAINER_DRAWN_FOCUS = new Set(['components/composer/ComposerCard.tsx']);

  it('every outline-none carries the focus-ring utility', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const rel = file.slice(SRC.length + 1);
      if (CONTAINER_DRAWN_FOCUS.has(rel)) continue;
      readFileSync(file, 'utf-8')
        .split('\n')
        .forEach((line, i) => {
          if (!/\boutline-none\b/.test(line)) return;
          if (/\bfocus-ring\b/.test(line)) return;
          offenders.push(`${rel}:${i + 1}: ${line.trim()}`);
        });
    }
    expect(offenders).toEqual([]);
  });
});

describe('type floor (R6)', () => {
  /**
   * The scale has three sizes and `text-floor` (11px) is the bottom of it.
   * An arbitrary `text-[10px]` or `text-[10.5px]` goes below the floor.
   *
   * The test reads the NUMBER instead of a list of bad literals. A list
   * catches `text-[10px]` and lets `text-[10.5px]` through, which is how
   * the palette footer stayed under the floor.
   */
  const FLOOR_PX = 11;
  const ARBITRARY_PX = /text-\[(\d+(?:\.\d+)?)px\]/g;

  it('no component sets a size below the 11px floor', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      readFileSync(file, 'utf-8')
        .split('\n')
        .forEach((line, i) => {
          for (const m of line.matchAll(ARBITRARY_PX)) {
            if (parseFloat(m[1]) < FLOOR_PX) {
              offenders.push(`${file.slice(SRC.length + 1)}:${i + 1}: ${line.trim()}`);
              break;
            }
          }
        });
    }
    expect(offenders).toEqual([]);
  });
});

describe('index.css import order', () => {
  // Tailwind v4 drops an @import that follows any other at-rule, and it does
  // so without an error. The four lane stylesheets vanished from the bundle
  // once because they sat below @plugin. Every @import must come first.
  it('every @import precedes the first @plugin or @theme', () => {
    const css = readFileSync(join(SRC, 'index.css'), 'utf-8');
    const lines = css.split('\n');
    // A bare `@layer a, b;` STATEMENT may precede imports (it fixes the
    // layer order); a `@layer x {` block may not.
    const firstOther = lines.findIndex((l) => /^\s*@(plugin|theme|custom-variant)\b/.test(l) || /^\s*@layer\b[^;]*\{/.test(l));
    const lateImports = lines
      .map((l, i) => ({ l, i }))
      .filter(({ l, i }) => /^\s*@import\b/.test(l) && i > firstOther);
    expect(lateImports.map(({ i, l }) => `${i + 1}: ${l.trim()}`)).toEqual([]);
  });
});

describe('no raw visual value in a component (token contract)', () => {
  /**
   * A web UI plugin restyles the app by setting `--cru-*` custom properties
   * (docs/Help/Extending/Web Theme Tokens.md). That only works while the app
   * itself reads those properties. One `text-[13px]` or one `'#e0653a'` is a
   * value the plugin cannot reach, and the plugin author has no way to find
   * out which one it was.
   *
   * THE GATE COVERS WHAT THE CONTRACT PROMISES, and no more. The contract
   * publishes colour, radius, type size, row height, elevation and three
   * measures. It publishes NO layout: it says so in as many words, and it
   * publishes no spacing family either, because the app spends space through
   * Tailwind's own `p-2` scale. So a `max-w-[140px]` that clamps a truncating
   * label is not an offender — there is no token it could read, and inventing
   * thirty of them would publish thirty names a plugin must never set.
   *
   * `vh`, `vw`, `%`, `ch` and `em` are absent from every pattern for the same
   * reason in a different shape: each is relative to something the token
   * already sets, so an `em` heading step already follows the type token.
   */

  /** An arbitrary value on a utility that carries visual IDENTITY.
   *  The lookbehind rejects a HYPHEN as well as a word character, so `h-[…]`
   *  does not match inside `min-h-[…]`: a minimum height is a layout floor,
   *  and `h-[…]` on its own is a row height, which the contract publishes. */
  const IDENTITY_UTILITY =
    /(?<![\w-])(?:text|rounded|leading|h|tracking|p|px|py|pt|pb|pl|pr|m|mx|my|mt|mb|ml|mr|gap|gap-x|gap-y)-\[-?[0-9.]+(?:px|rem)\]/;

  /** A colour, written anywhere at all. */
  const ANY_COLOR = /\[(?:#[0-9a-fA-F]{3,8}|rgba?\([^\]]*\))\]|['"`]#[0-9a-fA-F]{3,8}['"`]/;

  /** CodeMirror takes a style OBJECT, so its identity values are properties
   *  rather than classes. The same two families, the same floor. */
  const CSS_IN_JS_IDENTITY =
    /\b(?:fontSize|borderRadius|lineHeight)\s*:\s*['"`]-?[0-9.]+(?:px|rem)['"`]/;

  /**
   * A file whose raw values are GEOMETRY, not identity. A plugin that changed
   * one would break the layout rather than restyle it. Every entry carries
   * its reason, because a bare list of paths rots into a list of excuses.
   */
  const RAW_VALUE_ALLOWED = new Map<string, string>([
    // A canvas cannot follow a custom property: xterm and the graph both
    // paint literal colours. Each reads the token with getComputedStyle and
    // keeps the literal only as the fallback. `fallback sits beside its
    // token` below proves that pairing rather than trusting it.
    ['components/TerminalPanel.tsx', 'xterm canvas: token read with a fallback'],
    ['components/graph/GraphPanel.tsx', 'graph canvas: token read with a fallback'],
    ['lib/canvas-types.ts', 'canvas slots: var() with a fallback, pinned by canvas-viewport.test.ts'],

    // Language identity, not app theme. Go blue and Rust ochre are the same
    // colour in every theme and in every editor that draws them.
    ['lib/file-icons.ts', 'language brand colours'],

    // A JSON web-app manifest. The browser reads it before any CSS loads, so
    // a custom property cannot reach it.
    ['pwa-options.ts', 'PWA manifest colours'],

    // Window geometry: the grab strip around an edge, and the ribbon and
    // header that a maximized window has to clear.
    ['components/windowing/FloatingWindow.tsx', 'resize handle and maximize insets'],
    // A first-open panel size that the user then drags.
    ['components/windowing/EdgePanel.tsx', 'default panel width and height'],
    // The minimum a pointer can hit on a split drop zone.
    ['components/windowing/Pane.tsx', 'drop zone minimum'],

    // One device pixel.
    ['components/files/FileTreeNode.tsx', 'one-pixel indent guide'],
    ['components/shell/SessionStatusDot.tsx', 'one-pixel dot border'],

    // A prop default that the caller overrides.
    ['components/editor/MarkdownPreview.tsx', 'max-width prop default'],
  ]);

  it('every allow-list entry names a file that exists', () => {
    // An entry for a deleted file is a hole nobody can see.
    const present = new Set(walk(SRC).map((f) => f.slice(SRC.length + 1)));
    expect([...RAW_VALUE_ALLOWED.keys()].filter((k) => !present.has(k))).toEqual([]);
  });

  it('no component hard-codes a colour, a radius, a type size or a row height', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const rel = file.slice(SRC.length + 1);
      if (RAW_VALUE_ALLOWED.has(rel)) continue;
      readFileSync(file, 'utf-8')
        .split('\n')
        .forEach((line, i) => {
          if (line.trimStart().startsWith('*') || line.trimStart().startsWith('//')) return;
          if (IDENTITY_UTILITY.test(line) || ANY_COLOR.test(line) || CSS_IN_JS_IDENTITY.test(line)) {
            offenders.push(`${rel}:${i + 1}: ${line.trim()}`);
          }
        });
    }
    expect(offenders).toEqual([]);
  });

  it('every canvas colour literal is paired with the token it falls back to', () => {
    /**
     * The allow-list entry for a canvas file is a PROMISE that every literal
     * there is a fallback, never a value. Test the promise, by KEY.
     *
     * A line-level test is not enough. `TerminalPanel` writes the read and the
     * fallback on one line, but `GraphPanel` keeps the literals in a
     * `GRAPH_COLOR_FALLBACK` record and the token names in `readGraphColors`,
     * two lines apart. An exemption broad enough to admit the record also
     * admits `red: '#e8746e'` with no token anywhere, which is the failure
     * this test exists to produce. So: collect the keys that carry a hex, and
     * collect the keys that name a `--cru-` token, and require the first set
     * to be inside the second.
     */
    const KEYED_HEX = /^\s*(\w+):\s*[^\n]*['"`]#[0-9a-fA-F]{3,8}['"`]/;
    const KEYED_TOKEN = /^\s*(\w+):\s*[^\n]*(--cru-[a-z0-9-]+)/;
    const offenders: string[] = [];
    for (const rel of ['components/TerminalPanel.tsx', 'components/graph/GraphPanel.tsx']) {
      const lines = read(rel).split('\n').filter((l) => {
        const t = l.trimStart();
        return !t.startsWith('*') && !t.startsWith('//');
      });
      const tokened = new Set(lines.map((l) => KEYED_TOKEN.exec(l)?.[1]).filter(Boolean));
      for (const line of lines) {
        const key = KEYED_HEX.exec(line)?.[1];
        if (key && !tokened.has(key)) offenders.push(`${rel}: ${line.trim()}`);
      }
      expect(tokened.size, `${rel} names no --cru-* token at all`).toBeGreaterThan(0);
    }
    expect(offenders).toEqual([]);
  });

  it('no component keeps a hex fallback inside a var()', () => {
    // `var(--color-muted, #928d99)` is a SECOND copy of the value, and four of
    // them had already drifted from the token they shadow: muted was #9f9ba5
    // and hairline-strong was #322f38. The stylesheet always loads before a
    // component mounts, so the fallback buys nothing and can only lie.
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const rel = file.slice(SRC.length + 1);
      if (rel === 'lib/canvas-types.ts') continue; // pinned by canvas-viewport.test.ts
      readFileSync(file, 'utf-8')
        .split('\n')
        .forEach((line, i) => {
          if (/var\(\s*--[a-z0-9-]+\s*,\s*#[0-9a-fA-F]{3,8}\s*\)/.test(line)) {
            offenders.push(`${rel}:${i + 1}: ${line.trim()}`);
          }
        });
    }
    expect(offenders).toEqual([]);
  });
});

describe('a size that a reader scales is declared in rem (token contract)', () => {
  /**
   * A browser's font-size preference multiplies the root font size. A `rem`
   * follows that multiplier; a `px` does not. So a type size, a row height or
   * a measure written in `px` freezes the app at 16px for every reader who
   * asked for something else, and no plugin stylesheet can unfreeze it.
   *
   * The gate reads the PARSED contract, not the source text. A grep for
   * `13px` goes green the moment someone writes `13.0px`, and it can never
   * fail for the reason it was written.
   *
   * At a 16px root each converted value is the same number of device pixels
   * as the `px` it replaced, so this change moved no edge.
   */

  /** A family whose members scale with the reader. */
  const SCALING_FAMILY = /^--cru-(?:font|row|measure)-/;

  /**
   * A token that is `px` ON PURPOSE. Each entry gives the reason, because a
   * bare list of names rots into a list of excuses.
   *
   * Every OTHER px-by-design token sits outside the three scaling families
   * and this gate never sees it: `--cru-radius-*` draws a corner on a box,
   * and `--cru-shadow-*` offsets and blurs a shadow. A corner and a shadow
   * belong to the box, not to the text inside it, so neither follows the
   * reader.
   */
  const PX_BY_DESIGN = new Map<string, string>([
    [
      '--cru-row-touch',
      'a touch target floor: 44px is the minimum a finger can hit, and a reader who LOWERS the font size must not lose the target',
    ],
  ]);

  const PX_VALUE = /(?:^|[\s,(])-?[0-9.]+px\b/;

  it('every allow-list entry names a token the contract declares', () => {
    // An entry for a token that no longer exists is a hole nobody can see.
    for (const name of PX_BY_DESIGN.keys()) {
      expect(contractDark.has(name), `${name} is allow-listed but undeclared`).toBe(true);
    }
  });

  it('no --cru-font-*, --cru-row-* or --cru-measure-* token carries a px value', () => {
    const offenders: string[] = [];
    for (const contract of [contractDark, contractLight]) {
      for (const [name, value] of contract) {
        if (!SCALING_FAMILY.test(name)) continue;
        if (PX_BY_DESIGN.has(name)) continue;
        if (PX_VALUE.test(value)) offenders.push(`${name}: ${value}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('the touch row keeps its 44px floor', () => {
    // The allow-list says 44px is deliberate. Prove that it is still 44px:
    // an entry that excuses any value at all excuses the value going to 20px.
    expect(resolveToken(contractDark, '--cru-row-touch')).toBe('44px');
  });
});
