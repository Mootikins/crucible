import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { TreeSection } from '../TreeSection';

describe('TreeSection — the header', () => {
  it('is a whole number of pixels tall, so the rows under it land on whole pixels', () => {
    // 12 + 16 + 4 = 32; a 16.5px line put every row under it on a half pixel.
    render(() => (
      <TreeSection label="Inbox" testid="inbox" open onToggle={() => {}} count={2}>
        <div />
      </TreeSection>
    ));
    const header = screen.getByRole('button', { name: /Inbox/ });
    expect(header.className).toContain('leading-4');
  });

  it('draws its chevron in the 14 px slot every tier of the rail uses', () => {
    render(() => (
      <TreeSection label="Inbox" testid="inbox" open onToggle={() => {}} count={2}>
        <div />
      </TreeSection>
    ));
    const chevron = screen.getByRole('button', { name: /Inbox/ }).querySelector('svg')!;
    expect(chevron.getAttribute('class')).toContain('w-3.5');
  });
});
