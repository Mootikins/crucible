import { EditorView } from '@codemirror/view';
import { defaultHighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import type { Extension } from '@codemirror/state';
import type { Theme } from '@/lib/theme';

/**
 * Shell-aligned editor chrome, layered over the syntax colors of the theme.
 *
 * A theme ships its own panel background — oneDark's blue-grey #282c34 — that
 * clashes with the shell. This overrides the chrome (background, gutters,
 * active line) to the shell tokens, which follow the light/dark attribute on
 * their own, and keeps only the theme's token highlighting. It must come
 * BEFORE the syntax theme in the extension list: earlier extensions take
 * precedence in CM6.
 *
 * `dark` is not cosmetic. CodeMirror derives the selection layer, the drop
 * cursor and the panel shadows from it, so a dark flag on a light ground gives
 * an invisible selection.
 */
const chrome = (dark: boolean) =>
  EditorView.theme(
    {
      '&': { backgroundColor: 'var(--color-shell-panel)' },
      '.cm-gutters': {
        backgroundColor: 'var(--color-shell-panel)',
        borderRight: '1px solid var(--color-hairline)',
      },
      '.cm-activeLine': { backgroundColor: 'var(--color-hover-wash)' },
      '.cm-activeLineGutter': { backgroundColor: 'var(--color-hover-wash)' },
    },
    { dark },
  );

const crucibleEditorChromeDark = chrome(true);
const crucibleEditorChromeLight = chrome(false);

/**
 * Chrome plus syntax colors for one theme.
 *
 * Light uses CodeMirror's own `defaultHighlightStyle` rather than a hand-mixed
 * palette: it is the highlight style the library ships for a light ground, so
 * it is already contrast-checked, and it needs no new dependency.
 *
 * CodeMirror compiles a theme into a StyleModule at configuration time, so a
 * `var(--color-…)` cannot carry the syntax colors across a theme switch. The
 * editor swaps this whole extension in a compartment instead.
 */
export function editorThemeExtension(theme: Theme): Extension {
  return theme === 'light'
    ? [crucibleEditorChromeLight, syntaxHighlighting(defaultHighlightStyle)]
    : [crucibleEditorChromeDark, oneDark];
}
