// src/lib/markdown-path.test.ts
//
// The rows mirror the table in `crates/crucible-core/src/kiln.rs`'s tests. If a
// row changes here it must change there too — see markdown-path.ts.
import { describe, it, expect } from 'vitest';
import { isMarkdownPath, noteStem } from './markdown-path';

describe('isMarkdownPath', () => {
  const cases: Array<[path: string, expected: boolean]> = [
    ['a.md', true],
    ['a.MD', true],
    ['a.markdown', true],
    ['a.MARKDOWN', true],
    ['a.Markdown', true],
    ['notes/Reading List.markdown', true],
    // A dotfile with a real extension is still a note.
    ['.hidden.md', true],
    // MDX/MDC embed JSX; our parser would render it as prose.
    ['a.mdx', false],
    ['a.mdc', false],
    // `.txt` is not a note (kiln.rs classifies it Asset).
    ['a.txt', false],
    ['a.canvas', false],
    ['img.png', false],
    ['noext', false],
    // Only the LAST extension counts, so a backup is an asset.
    ['notes.md.bak', false],
    // A bare dotfile has no extension at all.
    ['.md', false],
    ['', false],
  ];

  for (const [path, expected] of cases) {
    it(`${JSON.stringify(path)} → ${expected}`, () => {
      expect(isMarkdownPath(path)).toBe(expected);
    });
  }
});

describe('noteStem', () => {
  it('strips either markdown extension, in any case', () => {
    expect(noteStem('a.md')).toBe('a');
    expect(noteStem('a.MD')).toBe('a');
    expect(noteStem('Reading List.markdown')).toBe('Reading List');
    expect(noteStem('Reading List.MARKDOWN')).toBe('Reading List');
  });

  it('leaves non-markdown names alone', () => {
    expect(noteStem('main.rs')).toBe('main.rs');
    expect(noteStem('a.mdx')).toBe('a.mdx');
    expect(noteStem('notes.md.bak')).toBe('notes.md.bak');
  });

  it('keeps dots that are part of the name', () => {
    expect(noteStem('v1.2.md')).toBe('v1.2');
  });
});
