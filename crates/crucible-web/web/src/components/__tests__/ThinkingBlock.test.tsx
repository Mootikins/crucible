import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';

// The thinking block's whole contract lives in three states a reader moves
// through: the model is reasoning (content streams in, visible as it lands),
// the model finished (the block folds back to one summary line), and the
// reader overrides the fold (their click wins until the next stream).
import { ThinkingBlock } from '../ThinkingBlock';

/** The collapsible wrapper exposes the fold through grid-template-rows:
 *  `1fr` expanded, `0fr` collapsed. jsdom does no layout, so the style is
 *  the only observable of the open/closed state. */
const foldOf = (root: HTMLElement): string => {
  const wrapper = root.querySelector('div.grid');
  expect(wrapper, 'collapsible wrapper').not.toBeNull();
  return (wrapper as HTMLElement).style.getPropertyValue('grid-template-rows');
};

describe('ThinkingBlock — streaming', () => {
  it('is expanded while the model is reasoning, with no click', () => {
    vi.useFakeTimers();
    try {
      const { container } = render(() => (
        <ThinkingBlock content="working through the options" isStreaming={true} />
      ));
      expect(foldOf(container)).toBe('1fr');
      // The reveal buffer starts empty and drips the words out on its own
      // cadence — a burst must not paste itself in whole.
      expect(container.textContent).not.toContain('options');
      vi.advanceTimersByTime(500);
      expect(container.textContent).toContain('working through the options');
    } finally {
      vi.useRealTimers();
    }
  });

  it('reveals words as individually fading spans while streaming', () => {
    vi.useFakeTimers();
    try {
      const { container } = render(() => (
        <ThinkingBlock content="alpha beta gamma delta" isStreaming={true} />
      ));
      expect(container.textContent).not.toContain('alpha');
      vi.advanceTimersByTime(60);
      const mid = container.textContent ?? '';
      // Partway through the reveal some words are visible but not all.
      expect(mid).not.toContain('delta');
      vi.advanceTimersByTime(2000);
      // Fully revealed, and the fade tail still renders as animated spans.
      expect(container.textContent).toContain('alpha beta gamma delta');
      expect(container.querySelectorAll('.thinking-word-in').length).toBeGreaterThan(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('reveals the whole content at once when the block is not streaming', () => {
    const { container } = render(() => (
      <ThinkingBlock content="a finished reasoning block" isStreaming={false} tokenCount={6} />
    ));
    // No reveal delay for history: the full text is there on first paint.
    expect(container.textContent).toContain('a finished reasoning block');
    expect(container.querySelectorAll('.thinking-word-in')).toHaveLength(0);
  });

  it('shows a stream caret after the reasoning text while streaming', () => {
    const { container } = render(() => (
      <ThinkingBlock content="mid-reasoning" isStreaming={true} />
    ));
    // Same cadence as the text stream caret (cru-caret); addressed by testid
    // so the animation's class list can move without breaking the contract.
    expect(container.querySelector('[data-testid="think-stream-caret"]')).not.toBeNull();
  });

  it('carries no wave dots once the thinking is streaming=false — a finished block is quiet', () => {
    const { container } = render(() => (
      <ThinkingBlock content="done reasoning" isStreaming={false} tokenCount={42} />
    ));
    expect(container.querySelectorAll('.cru-think-dot')).toHaveLength(0);
    expect(screen.getByText('Thought for ~42 tokens')).toBeInTheDocument();
  });
});

describe('ThinkingBlock — completion', () => {
  it('folds back to the summary line when the stream ends', () => {
    const [streaming, setStreaming] = createSignal(true);
    const { container } = render(() => (
      <ThinkingBlock content="reasoning" isStreaming={streaming()} tokenCount={9} />
    ));
    expect(foldOf(container)).toBe('1fr');

    setStreaming(false);
    expect(foldOf(container)).toBe('0fr');
    expect(screen.getByText('Thought for ~9 tokens')).toBeInTheDocument();
    expect(container.querySelector('[data-testid="think-stream-caret"]')).toBeNull();
  });
});

describe('ThinkingBlock — reader override', () => {
  it('a click collapses mid-stream, and a second click reopens it', () => {
    const { container } = render(() => (
      <ThinkingBlock content="reasoning" isStreaming={true} />
    ));
    fireEvent.click(screen.getByRole('button'));
    expect(foldOf(container)).toBe('0fr');
    fireEvent.click(screen.getByRole('button'));
    expect(foldOf(container)).toBe('1fr');
  });

  it('the reader can reopen a finished block', () => {
    const { container } = render(() => (
      <ThinkingBlock content="reasoning" isStreaming={false} tokenCount={9} />
    ));
    expect(foldOf(container)).toBe('0fr');
    fireEvent.click(screen.getByRole('button'));
    expect(foldOf(container)).toBe('1fr');
    expect(screen.getByText('reasoning')).toBeInTheDocument();
  });

  it('a completed block folds again after the reader reopened it and the header was re-rendered', () => {
    // Reader override lives until the next stream, not forever: reopening a
    // finished block must not pin the block open against the next turn's
    // completion. (The completion transition is what resets the override.)
    const [streaming, setStreaming] = createSignal(false);
    const { container } = render(() => (
      <ThinkingBlock content="reasoning" isStreaming={streaming()} tokenCount={9} />
    ));
    fireEvent.click(screen.getByRole('button'));
    expect(foldOf(container)).toBe('1fr');

    // The next turn's thinking starts (override gives way to streaming) and
    // ends again: the block must be folded by the completion, not left open.
    setStreaming(true);
    expect(foldOf(container)).toBe('1fr');
    setStreaming(false);
    expect(foldOf(container)).toBe('0fr');
  });
});
