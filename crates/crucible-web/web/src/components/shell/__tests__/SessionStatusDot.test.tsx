import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { SessionStatusDot } from '../SessionStatusDot';

/** The dot's own inline style — the only place its shape is decided. */
const dotStyle = (container: HTMLElement) =>
  (container.querySelector('[data-testid="session-status-dot"]') as HTMLElement).style;

describe('SessionStatusDot', () => {
  it('fills the dot while the machine is busy', () => {
    const { container } = render(() => <SessionStatusDot status="working" />);
    const style = dotStyle(container);
    expect(style.background).not.toBe('transparent');
    expect(style.borderStyle).toBe('none');
  });

  it('rings the dot hollow while YOU are the blocker', () => {
    const { container } = render(() => <SessionStatusDot status="waiting" />);
    const style = dotStyle(container);
    // Hollow is the whole signal: a filled waiting dot is a working dot.
    expect(style.background).toBe('transparent');
    expect(style.borderWidth).toBe('1.5px');
    expect(style.borderStyle).toBe('solid');
  });

  it('separates working from waiting by FILL first, hue second', () => {
    const { container: working } = render(() => <SessionStatusDot status="working" />);
    const { container: waiting } = render(() => <SessionStatusDot status="waiting" />);
    // Fill vs ring carries the meaning on its own, so it survives at 7px and
    // on a colour-blind screen. The hues then reinforce it with the words the
    // rest of the app already uses.
    expect(dotStyle(working).background).not.toBe('transparent');
    expect(dotStyle(working).borderStyle).toBe('none');
    expect(dotStyle(waiting).background).toBe('transparent');
    expect(dotStyle(waiting).borderStyle).toBe('solid');
    expect(dotStyle(working).background).not.toBe(dotStyle(waiting).background);
  });

  it('speaks the status vocabulary the rest of the app already uses', () => {
    const { container: working } = render(() => <SessionStatusDot status="working" />);
    const { container: waiting } = render(() => <SessionStatusDot status="waiting" />);
    // `attention` is "waiting on you" in the Inbox, the Changes panel and the
    // status chips; `ok` is a live stream. A private near-amber beside
    // `attention` could not read as a second meaning at this size.
    expect(dotStyle(working).background).toContain('--color-ok');
    expect(dotStyle(waiting).borderColor).toContain('--color-attention');
  });

  it('drops every status colour when nothing is happening', () => {
    const { container } = render(() => <SessionStatusDot status="idle" />);
    const style = dotStyle(container);
    expect(style.background).not.toContain('--color-ok');
    expect(style.background).not.toContain('--color-attention');
    expect(style.borderStyle).toBe('none');
  });

  it('names its state for a screen reader when asked to', () => {
    const { container } = render(() => <SessionStatusDot status="waiting" labelled />);
    const dot = container.querySelector('[data-testid="session-status-dot"]')!;
    expect(dot.getAttribute('aria-label')).toBe('Waiting for you');
    expect(dot.getAttribute('data-status')).toBe('waiting');
  });

  it('stays silent beside a row that already has a name', () => {
    const { container } = render(() => <SessionStatusDot status="idle" />);
    const dot = container.querySelector('[data-testid="session-status-dot"]')!;
    // A 200-row list would otherwise announce "Idle, image" 200 times and pop
    // a tooltip on every hover. Decoration beside a title, not a second name.
    expect(dot.getAttribute('aria-hidden')).toBe('true');
    expect(dot.getAttribute('aria-label')).toBeNull();
    expect(dot.getAttribute('title')).toBeNull();
  });
});
