import { describe, it, expect } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { editorThemeExtension } from '../editor-theme';

const stateFor = (theme: 'dark' | 'light') =>
  EditorState.create({ extensions: editorThemeExtension(theme) });

describe('editorThemeExtension', () => {
  it('declares its darkness to CodeMirror', () => {
    // Not cosmetic: CodeMirror builds the selection layer, the drop cursor and
    // the panel shadows from this flag, so the dark one on a light ground
    // gives a selection you cannot see.
    expect(stateFor('dark').facet(EditorView.darkTheme)).toBe(true);
    expect(stateFor('light').facet(EditorView.darkTheme)).toBe(false);
  });

  it('installs a different set of style modules per theme', () => {
    // A syntax theme is a compiled StyleModule, not CSS custom properties, so
    // the two themes cannot share one — if they did, light would paint One
    // Dark's low-contrast greys onto a white ground.
    const dark = stateFor('dark').facet(EditorView.styleModule);
    const light = stateFor('light').facet(EditorView.styleModule);
    expect(dark.length).toBeGreaterThan(0);
    expect(light.length).toBeGreaterThan(0);
    expect(dark.filter((m) => light.includes(m))).toEqual([]);
  });

  it.each(['dark', 'light'] as const)('never underlines a heading in %s', (theme) => {
    // `defaultHighlightStyle` ships `{tag: heading, textDecoration: underline,
    // fontWeight: bold}` and One Dark ships no underline, so the light editor
    // underlined every markdown heading and the dark one did not. Underline
    // WITH bold in one rule is the heading spec's signature in both themes;
    // a link keeps its own underline and carries no weight.
    const css = stateFor(theme)
      .facet(EditorView.styleModule)
      .map((m) => m.getRules())
      .join('\n');

    for (const rule of css.split('\n')) {
      const underlined = /text-decoration:\s*underline/.test(rule);
      const bold = /font-weight:\s*bold/.test(rule);
      expect(underlined && bold).toBe(false);
    }
  });
});
