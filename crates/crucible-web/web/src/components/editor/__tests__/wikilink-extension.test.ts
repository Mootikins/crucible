import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import {
  wikilinkNavigation,
  wikilinkTargetAt,
  followWikilinkAtCursor,
} from '../wikilink-extension';
import { editorThemeExtension } from '../editor-theme';
import { darkTokens, lightTokens, resolveToken, tokenReferenceIn } from '@/test-utils/css-tokens';

function makeView(doc: string, onFollow: (t: string) => void = () => {}): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  return new EditorView({
    state: EditorState.create({ doc, extensions: [wikilinkNavigation(onFollow)] }),
    parent,
  });
}

let views: EditorView[] = [];

function track(view: EditorView): EditorView {
  views.push(view);
  return view;
}

beforeEach(() => {
  views = [];
});

afterEach(() => {
  views.forEach((v) => v.destroy());
  document.body.innerHTML = '';
});

describe('wikilink decorations', () => {
  it('marks [[wikilinks]] with .cm-wikilink and a data-note attribute', () => {
    const view = track(makeView('See [[My Note]] for details.'));
    const link = view.dom.querySelector('.cm-wikilink');
    expect(link).not.toBeNull();
    expect(link!.getAttribute('data-note')).toBe('My Note');
    expect(link!.textContent).toBe('[[My Note]]');
  });

  it('resolves aliased and fragmented links to the bare target', () => {
    const view = track(makeView('[[Target|shown]] and [[Other#Heading]]'));
    const links = view.dom.querySelectorAll('.cm-wikilink');
    expect(links).toHaveLength(2);
    expect(links[0].getAttribute('data-note')).toBe('Target');
    expect(links[1].getAttribute('data-note')).toBe('Other');
  });

  it('decorates links added by later edits', () => {
    const view = track(makeView('plain text'));
    expect(view.dom.querySelector('.cm-wikilink')).toBeNull();
    view.dispatch({ changes: { from: 0, insert: '[[New Note]] ' } });
    expect(view.dom.querySelector('.cm-wikilink')?.getAttribute('data-note')).toBe('New Note');
  });
});

describe('code contexts get no wikilink treatment (needs markdown grammar)', () => {
  const TOML_DOC = 'Prose [[Real]] link.\n\n```toml\n[[mcp.upstreams]]\n```\n';

  function makeMarkdownView(doc: string): EditorView {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    return new EditorView({
      state: EditorState.create({
        doc,
        extensions: [markdown({ base: markdownLanguage }), wikilinkNavigation(() => {})],
      }),
      parent,
    });
  }

  it('no pill on TOML [[table]] headers inside fences; prose links still pill', () => {
    const view = track(makeMarkdownView(TOML_DOC));
    const pills = [...view.dom.querySelectorAll('.cm-wikilink')].map((el) =>
      el.getAttribute('data-note'),
    );
    expect(pills).toEqual(['Real']);
  });

  it('wikilinkTargetAt returns null inside a fence (Mod-Enter no-op)', () => {
    const view = track(makeMarkdownView(TOML_DOC));
    const inFence = TOML_DOC.indexOf('mcp.upstreams');
    expect(wikilinkTargetAt(view.state, inFence)).toBeNull();
    const inProse = TOML_DOC.indexOf('Real');
    expect(wikilinkTargetAt(view.state, inProse)).toBe('Real');
  });
});

describe('wikilinkTargetAt', () => {
  it('finds the target when the position is inside a link', () => {
    const view = track(makeView('before [[My Note]] after'));
    // Position inside "[[My Note]]" (starts at 7).
    expect(wikilinkTargetAt(view.state, 10)).toBe('My Note');
  });

  it('returns null outside links', () => {
    const view = track(makeView('before [[My Note]] after'));
    expect(wikilinkTargetAt(view.state, 2)).toBeNull();
    expect(wikilinkTargetAt(view.state, view.state.doc.length)).toBeNull();
  });
});

describe('follow gestures', () => {
  it('Mod-Enter command follows the link under the cursor', () => {
    const onFollow = vi.fn();
    const view = track(makeView('go to [[My Note]] now', onFollow));
    view.dispatch({ selection: { anchor: 10 } });

    const handled = followWikilinkAtCursor(onFollow)(view);
    expect(handled).toBe(true);
    expect(onFollow).toHaveBeenCalledWith('My Note');
  });

  it('Mod-Enter command declines when the cursor is not in a link', () => {
    const onFollow = vi.fn();
    const view = track(makeView('go to [[My Note]] now', onFollow));
    view.dispatch({ selection: { anchor: 2 } });

    expect(followWikilinkAtCursor(onFollow)(view)).toBe(false);
    expect(onFollow).not.toHaveBeenCalled();
  });

  it('Ctrl+Click on a decorated link follows it', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const link = view.dom.querySelector('.cm-wikilink')!;

    link.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, ctrlKey: true }));
    expect(onFollow).toHaveBeenCalledWith('My Note');
  });

  it('plain click does not follow (text editing stays untouched)', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const link = view.dom.querySelector('.cm-wikilink')!;

    link.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(onFollow).not.toHaveBeenCalled();
  });
});

/**
 * The wikilink is the product's signature primitive, and the syntax theme is
 * the thing most likely to take it away: the markdown grammar highlights the
 * brackets and the name INSIDE the mark decoration, so a syntax colour lands
 * on a descendant of `.cm-wikilink` and wins there whatever the extension
 * order says.
 *
 * These assert the COMPUTED colour with the real editor theme installed, not
 * the presence of the class. jsdom runs the cascade but does not substitute
 * `var()`, so the computed value is the token NAME the cascade selected; the
 * name is then followed into `index.css` for the colour itself. A class-name
 * assertion would have passed throughout the bug this test exists to catch.
 */
describe('wikilink colour survives the syntax theme', () => {
  function makeThemedView(theme: 'dark' | 'light'): EditorView {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    return new EditorView({
      state: EditorState.create({
        doc: 'See [[My Note]] for details.',
        extensions: [
          markdown({ base: markdownLanguage }),
          wikilinkNavigation(() => {}),
          editorThemeExtension(theme),
        ],
      }),
      parent,
    });
  }

  /** Every element that carries a glyph of the link, innermost included. */
  function paintedParts(view: EditorView): Element[] {
    const pill = view.dom.querySelector('.cm-wikilink')!;
    return [pill, ...pill.querySelectorAll('*')];
  }

  it.each(['dark', 'light'] as const)('paints every part ember in %s', (theme) => {
    const view = track(makeThemedView(theme));
    const parts = paintedParts(view);
    // The nesting is the whole point: with no descendant there is nothing for
    // a syntax colour to land on, and this test would prove nothing.
    expect(parts.length).toBeGreaterThan(1);

    for (const part of parts) {
      const color = getComputedStyle(part).color;
      // `inherit` on a descendant IS the ember, handed down by the pill.
      if (color === 'inherit') continue;
      expect(tokenReferenceIn(color)).toBe('--color-primary');
    }
  });

  it('resolves to the ember of each theme, and the two differ', () => {
    const dark = resolveToken(darkTokens, '--color-primary');
    const light = resolveToken(lightTokens, '--color-primary');
    expect(dark).toBe('#e0653a');
    expect(light).toBe('#b04823');
    expect(light).not.toBe(dark);
  });

  it('clears the underline at rest, on the pill AND on its spans', () => {
    // The light theme underlined a wikilink and the dark theme did not:
    // `defaultHighlightStyle` tags `[[Note]]` as a link and underlines it,
    // and the line is drawn by a DESCENDANT span. A descendant cannot erase
    // a line an ancestor draws, and an ancestor cannot erase a descendant's,
    // so the rule must name the span for the two themes to agree.
    //
    // The RULE, not the computed style: jsdom applies no CodeMirror class to
    // the nested spans it did not build, so a computed-style assertion here
    // passes whatever the stylesheet says.
    track(makeThemedView('light'));
    const rules = [...document.head.querySelectorAll('style')]
      .flatMap((el) => [...(el.sheet?.cssRules ?? [])])
      .filter((r): r is CSSStyleRule => r instanceof CSSStyleRule)
      .filter((r) => r.selectorText.includes('.cm-wikilink') && !r.selectorText.includes(':hover'))
      .filter((r) => r.style.textDecoration === 'none');

    expect(rules.length).toBeGreaterThan(0);
    expect(rules.some((r) => r.selectorText.includes('.cm-wikilink span'))).toBe(true);
  });

  it('carries the follow affordance on hover', () => {
    const view = track(makeThemedView('dark'));
    const rules = [...document.head.querySelectorAll('style')]
      .flatMap((el) => [...(el.sheet?.cssRules ?? [])])
      .filter((r): r is CSSStyleRule => r instanceof CSSStyleRule)
      .filter((r) => r.selectorText.includes('.cm-wikilink') && r.selectorText.includes(':hover'));
    expect(rules.length).toBeGreaterThan(0);
    expect(rules.some((r) => r.style.textDecoration === 'underline')).toBe(true);
    expect(view.dom.querySelector('.cm-wikilink')).not.toBeNull();
  });
});
