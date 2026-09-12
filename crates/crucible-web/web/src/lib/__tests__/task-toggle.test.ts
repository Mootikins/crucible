import { describe, it, expect } from 'vitest';
import { applyTaskToggle, flipTaskLine, isTaskLine, taskEditForLine } from '@/lib/task-toggle';

describe('recognising a task line', () => {
  it('takes every list marker GFM allows', () => {
    for (const line of ['- [ ] a', '* [ ] a', '+ [ ] a', '1. [ ] a', '2) [ ] a']) {
      expect(isTaskLine(line), line).toBe(true);
    }
  });

  it('keeps indentation, so a nested task is still a task', () => {
    expect(isTaskLine('    - [x] nested')).toBe(true);
  });

  it('is not fooled by a bracket that is not a box', () => {
    for (const line of ['- [] a', '- [y] a', '- [ ]a', 'a - [ ] b', '[ ] no marker']) {
      expect(isTaskLine(line), line).toBe(false);
    }
  });
});

describe('flipping a box', () => {
  it('ticks an empty box and unticks a full one', () => {
    expect(flipTaskLine('- [ ] write it')).toBe('- [x] write it');
    expect(flipTaskLine('- [x] write it')).toBe('- [ ] write it');
    expect(flipTaskLine('- [X] write it')).toBe('- [ ] write it');
  });

  it('changes the box and NOTHING else', () => {
    // Indentation, marker, spacing and trailing content are the user's.
    expect(flipTaskLine('   * [ ]   spaced   out   ')).toBe('   * [x]   spaced   out   ');
    expect(flipTaskLine('1. [ ] ordered [x] with a bracket later')).toBe(
      '1. [x] ordered [x] with a bracket later',
    );
  });

  it('answers null for a line that is not a task', () => {
    expect(flipTaskLine('just prose')).toBeNull();
  });
});

describe('the anchored edit a tick becomes', () => {
  const note = '# Todo\n\n- [ ] alpha\n- [x] beta\n\nprose\n';

  it('anchors on the whole line and replaces only the box', () => {
    expect(taskEditForLine(note, 2)).toEqual({ expect: '- [ ] alpha', replace: '- [x] alpha' });
  });

  it('unticks too', () => {
    expect(taskEditForLine(note, 3)).toEqual({ expect: '- [x] beta', replace: '- [ ] beta' });
  });

  it('names no occurrence when the line is unique', () => {
    // An absent occurrence is the daemon's "exactly once", which is a
    // stronger check than naming index 0.
    expect(taskEditForLine(note, 2)).not.toHaveProperty('occurrence');
  });

  /**
   * A list of identical lines is ordinary. Without an occurrence the daemon
   * refuses the batch as ambiguous, and the tap would do nothing.
   */
  it('names which of several identical lines was tapped', () => {
    const repeated = '- [ ] ping\n- [ ] ping\n- [ ] ping\n';
    expect(taskEditForLine(repeated, 0)).toEqual({
      expect: '- [ ] ping',
      replace: '- [x] ping',
      occurrence: 0,
    });
    expect(taskEditForLine(repeated, 2)).toEqual({
      expect: '- [ ] ping',
      replace: '- [x] ping',
      occurrence: 2,
    });
  });

  it('counts only IDENTICAL lines, not every task line', () => {
    const mixed = '- [ ] a\n- [ ] b\n- [ ] a\n';
    expect(taskEditForLine(mixed, 2)).toMatchObject({ occurrence: 1 });
  });

  it('answers null for a line that is no longer a task', () => {
    expect(taskEditForLine(note, 5)).toBeNull();
    expect(taskEditForLine(note, 99)).toBeNull();
  });
});

describe('the optimistic local flip', () => {
  it('changes one line and leaves the rest byte-identical', () => {
    const note = '# Todo\n\n- [ ] alpha\n- [x] beta\n';
    expect(applyTaskToggle(note, 2)).toBe('# Todo\n\n- [x] alpha\n- [x] beta\n');
  });

  it('keeps a trailing newline exactly as it was', () => {
    expect(applyTaskToggle('- [ ] a', 0)).toBe('- [x] a');
    expect(applyTaskToggle('- [ ] a\n', 0)).toBe('- [x] a\n');
  });

  it('answers null rather than corrupting a line it does not understand', () => {
    expect(applyTaskToggle('prose\n', 0)).toBeNull();
  });
});
