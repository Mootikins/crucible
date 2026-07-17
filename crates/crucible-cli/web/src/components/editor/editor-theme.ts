import { EditorView } from '@codemirror/view';

/**
 * Shell-aligned editor chrome layered over oneDark's syntax colors.
 *
 * oneDark ships its own blue-grey panel background (#282c34) that clashes
 * with the near-black ember shell — this overrides the chrome (background,
 * gutters, active line) to the shell tokens while keeping oneDark's token
 * highlighting. Must come BEFORE oneDark in the extension list — earlier
 * extensions take precedence in CM6.
 */
export const crucibleEditorChrome = EditorView.theme(
  {
    '&': { backgroundColor: 'var(--color-shell-panel, #141318)' },
    '.cm-gutters': {
      backgroundColor: 'var(--color-shell-panel, #141318)',
      borderRight: '1px solid var(--color-hairline, #26252b)',
    },
    '.cm-activeLine': { backgroundColor: 'rgba(255, 255, 255, 0.03)' },
    '.cm-activeLineGutter': { backgroundColor: 'rgba(255, 255, 255, 0.05)' },
  },
  { dark: true },
);
