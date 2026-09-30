import { describe, expect, it } from 'vitest';
import { blocks } from '../blocks';

describe('assistant turn actions', () => {
  it('reserves no controls row while a permission request is pending', () => {
    const result = blocks([
      { t: 'user', text: 'Edit the note', time: 'now' },
      { t: 'text', md: 'I will edit it.' },
      { t: 'tool', id: 'edit', name: 'write_note', st: 'ask' },
    ]);
    expect(result.filter(b => b.kind === 'item' && b.copyText)).toEqual([]);
  });

  it('copies interim text and the final answer from one footer', () => {
    const result = blocks([
      { t: 'user', text: 'Edit the note', time: 'now' },
      { t: 'text', md: 'I will edit it.' },
      { t: 'tool', id: 'edit', name: 'write_note', st: 'ok' },
      { t: 'text', md: 'Done.', elapsed: '1s' },
      { t: 'record', text: 'Edits allowed for this session' },
    ]);
    expect(result.flatMap(b => b.kind === 'item' && b.copyText ? [b.copyText] : [])).toEqual(['I will edit it.\n\nDone.']);
  });

  it('keeps copy text inside each user turn', () => {
    const result = blocks([
      { t: 'user', text: 'First question', time: 'now' },
      { t: 'text', md: 'First answer.', elapsed: '1s' },
      { t: 'user', text: 'Second question', time: 'now' },
      { t: 'text', md: 'Second answer.', elapsed: '1s' },
    ]);
    expect(result.flatMap(b => b.kind === 'item' && b.copyText ? [b.copyText] : [])).toEqual(['First answer.', 'Second answer.']);
  });
});
