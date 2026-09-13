import { describe, it, expect } from 'vitest';
import { applyAnchoredEdits } from '@/lib/offline/fold';

const NOTE = ['# List', '- [ ] milk', '- [ ] eggs', '- [ ] milk', ''].join('\n');

describe('applyAnchoredEdits', () => {
  it('replaces the one matching line', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '- [ ] eggs', replace: '- [x] eggs' }]);
    expect(out).toEqual({ ok: true, text: NOTE.replace('- [ ] eggs', '- [x] eggs') });
  });

  it('replaces the nth matching line', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '- [ ] milk', replace: '- [x] milk', occurrence: 1 }]);
    expect(out).toEqual({
      ok: true,
      text: ['# List', '- [ ] milk', '- [ ] eggs', '- [x] milk', ''].join('\n'),
    });
  });

  // The daemon refuses this edit, so the fold refuses it too: a guess at the
  // first line would show text the daemon will never write.
  it('refuses an ambiguous match when no occurrence is named', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '- [ ] milk', replace: '- [x] milk' }]);
    expect(out).toEqual({ ok: false, index: 0 });
  });

  // The match is the whole line: a line that only contains the text is not it.
  it('matches a whole line, not a substring', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '[ ] eggs', replace: '[x] eggs' }]);
    expect(out).toEqual({ ok: false, index: 0 });
  });

  it('refuses when the line is absent', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '- [ ] bread', replace: '- [x] bread' }]);
    expect(out).toEqual({ ok: false, index: 0 });
  });

  it('refuses when the named occurrence does not exist', () => {
    const out = applyAnchoredEdits(NOTE, [{ expect: '- [ ] eggs', replace: '- [x] eggs', occurrence: 1 }]);
    expect(out).toEqual({ ok: false, index: 0 });
  });

  // Each edit sees the text the earlier edits produced, so a second tick on
  // the same line finds the ticked line, not the original.
  it('applies the edits in order against the running text', () => {
    const out = applyAnchoredEdits(NOTE, [
      { expect: '- [ ] eggs', replace: '- [x] eggs' },
      { expect: '- [x] eggs', replace: '- [ ] eggs' },
    ]);
    expect(out).toEqual({ ok: true, text: NOTE });
  });

  it('names the edit that failed, and applies nothing', () => {
    const out = applyAnchoredEdits(NOTE, [
      { expect: '- [ ] eggs', replace: '- [x] eggs' },
      { expect: '- [ ] bread', replace: '- [x] bread' },
    ]);
    expect(out).toEqual({ ok: false, index: 1 });
  });

  it('keeps a CRLF line ending on the line it replaces', () => {
    const crlf = '- [ ] milk\r\n- [ ] eggs\r\n';
    const out = applyAnchoredEdits(crlf, [{ expect: '- [ ] eggs', replace: '- [x] eggs' }]);
    expect(out).toEqual({ ok: true, text: '- [ ] milk\r\n- [x] eggs\r\n' });
  });
});
