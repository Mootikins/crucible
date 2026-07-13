import { describe, it, expect, vi } from 'vitest';
import { render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { EditorView } from '@codemirror/view';

import { CodeMirrorEditor, getLanguageExtension } from '../CodeMirrorEditor';

const findView = (container: HTMLElement): EditorView => {
  const content = container.querySelector('.cm-content');
  expect(content).not.toBeNull();
  const view = EditorView.findFromDOM(content as HTMLElement);
  expect(view).not.toBeNull();
  return view!;
};

describe('CodeMirrorEditor — programmatic sync vs user edits (bug 5)', () => {
  it('switching the content/path props does not fire onChange', () => {
    const onChange = vi.fn();
    const [content, setContent] = createSignal('# first file\n');
    const [path, setPath] = createSignal('/kiln/first.md');

    render(() => (
      <CodeMirrorEditor content={content()} path={path()} onChange={onChange} />
    ));

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
    ['/kiln/note.md', 'markdown'],
    ['/kiln/note.markdown', 'markdown'],
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
