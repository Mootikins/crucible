/**
 * The Extensions mode of the file tree: a note shows no label, and every
 * other file shows its extension in capitals.
 */
import { describe, expect, it } from 'vitest';
import { render } from '@solidjs/testing-library';
import { FileRow } from '../FileRow';
import { fileLabel } from '../fileLabel';

describe('fileLabel', () => {
  it('gives a note no extension label', () => {
    expect(fileLabel('Precognition.md')).toEqual({ title: 'Precognition' });
    // A dot inside a note name is not an extension.
    expect(fileLabel('Z.AI Setup.md')).toEqual({ title: 'Z.AI Setup' });
  });

  it('gives any other file its extension in capitals', () => {
    expect(fileLabel('Knowledge Map.canvas')).toEqual({ title: 'Knowledge Map', ext: 'CANVAS' });
    expect(fileLabel('flow.png')).toEqual({ title: 'flow', ext: 'PNG' });
  });
});

describe('FileRow', () => {
  const row = (name: string, labels: 'icons' | 'extensions') =>
    render(() => (
      <FileRow name={name} depth={0} labels={labels} current={false} pending={0} touched={false} color="red" onOpen={() => {}} />
    )).container;

  it('shows the extension label only on a file that is not a note', () => {
    expect(row('Knowledge Map.canvas', 'extensions').querySelector('.mk-ext')?.textContent).toBe('CANVAS');
    expect(row('Precognition.md', 'extensions').querySelector('.mk-ext')).toBeNull();
  });

  it('shows an icon, and no label, on every file in the Icons mode', () => {
    for (const name of ['Precognition.md', 'Knowledge Map.canvas']) {
      const el = row(name, 'icons');
      expect(el.querySelector('svg')).not.toBeNull();
      expect(el.querySelector('.mk-ext')).toBeNull();
    }
  });
});
