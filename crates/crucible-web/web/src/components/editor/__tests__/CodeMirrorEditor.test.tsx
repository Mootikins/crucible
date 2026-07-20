import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { EditorView } from '@codemirror/view';
import { LanguageDescription } from '@codemirror/language';
import { languages as codeLanguages } from '@codemirror/language-data';

import { CodeMirrorEditor, getLanguageExtension } from '../CodeMirrorEditor';

// EditorView holds real DOM listeners and observers; testing-library's cleanup
// unmounts the Solid tree but never calls view.destroy(). Track every view we
// resolve and tear them down so instances don't leak across tests.
const openViews: EditorView[] = [];

afterEach(() => {
  for (const view of openViews) view.destroy();
  openViews.length = 0;
});

const findView = (container: HTMLElement): EditorView => {
  const content = container.querySelector('.cm-content');
  expect(content).not.toBeNull();
  const view = EditorView.findFromDOM(content as HTMLElement);
  expect(view).not.toBeNull();
  openViews.push(view!);
  return view!;
};

describe('CodeMirrorEditor — programmatic sync vs user edits (bug 5)', () => {
  it('switching the content/path props does not fire onChange', () => {
    const onChange = vi.fn();
    const [content, setContent] = createSignal('# first file\n');
    const [path, setPath] = createSignal('/kiln/first.md');

    const { container } = render(() => (
      <CodeMirrorEditor content={content()} path={path()} onChange={onChange} />
    ));
    // Track the mounted view so afterEach can destroy it.
    findView(container);

    // Simulate the active-file switch: EditorPanel feeds the reused instance
    // the next file's content. This is a sync, not a user edit — it must not
    // mark the incoming file dirty.
    setPath('/kiln/second.md');
    setContent('# second file\n');

    expect(onChange).not.toHaveBeenCalled();
  });

  it('a user edit still fires onChange with the new doc', () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <CodeMirrorEditor content="hello" path="/kiln/note.md" onChange={onChange} />
    ));

    const view = findView(container);
    // An un-annotated transaction is what user typing produces.
    view.dispatch({ changes: { from: 5, insert: ' world' } });

    expect(onChange).toHaveBeenCalledWith('hello world');
  });
});

describe('getLanguageExtension — language coverage (bug 7)', () => {
  it.each([
    // Markdown is wrapped in yamlFrontmatter so a leading `---` block
    // parses as YAML — the outer language carries that name.
    ['/kiln/note.md', 'yaml-frontmatter'],
    ['/kiln/note.markdown', 'yaml-frontmatter'],
    ['/src/main.rs', 'rust'],
    ['/src/index.js', 'javascript'],
    ['/src/App.jsx', 'javascript'],
    ['/src/lib.ts', 'typescript'],
    ['/src/App.tsx', 'typescript'],
  ])('%s → %s highlighting', (path, langName) => {
    const ext = getLanguageExtension(path);
    expect(ext).not.toBeNull();
    expect(ext!.language.name).toBe(langName);
  });

  it('returns null for unknown extensions', () => {
    expect(getLanguageExtension('/kiln/data.csv')).toBeNull();
    expect(getLanguageExtension('/kiln/noext')).toBeNull();
  });
});

describe('lazy language resolution (language-data)', () => {
  it.each([
    ['/proj/crucible.toml', 'TOML'],
    ['/proj/Cargo.toml', 'TOML'],
    ['/proj/package.json', 'JSON'],
    ['/proj/script.py', 'Python'],
    ['/proj/main.go', 'Go'],
    ['/proj/styles.css', 'CSS'],
    ['/proj/index.html', 'HTML'],
    ['/proj/run.sh', 'Shell'],
    ['/proj/config.yaml', 'YAML'],
  ])('%s resolves to the %s grammar via matchFilename', (path, langName) => {
    const filename = path.split('/').pop()!;
    const desc = LanguageDescription.matchFilename(codeLanguages, filename);
    expect(desc?.name).toBe(langName);
  });

  it('resolves nothing for a plain-text file with no known grammar', () => {
    expect(LanguageDescription.matchFilename(codeLanguages, 'notes.txt')).toBeNull();
  });
});
