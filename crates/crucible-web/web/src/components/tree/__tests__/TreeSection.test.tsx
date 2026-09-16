import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
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
    expect(header.className).toContain('h-(--cru-row-sm)');
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

  it('puts its actions before its count, not after', () => {
    const onToggle = vi.fn();
    render(() => (
      <TreeSection
        label="Projects"
        testid="projects"
        open
        onToggle={onToggle}
        count={3}
        actions={<button data-testid="project-menu">menu</button>}
      >
        <div />
      </TreeSection>
    ));

    const row = screen.getByTestId('project-menu').closest('div.flex.items-center')!;
    const labelEl = row.querySelector('span')!;
    const actionsEl = screen.getByTestId('project-menu');
    const countEl = row.querySelector('span[aria-hidden="true"]')!;

    // Label, then actions, then the visible count — in that DOM order.
    const order = Array.from(row.querySelectorAll('*'));
    expect(order.indexOf(labelEl)).toBeLessThan(order.indexOf(actionsEl));
    expect(order.indexOf(actionsEl)).toBeLessThan(order.indexOf(countEl));

    // The toggle button keeps "3" in its accessible name.
    const toggle = screen.getByRole('button', { name: /Projects.*3/ });
    expect(toggle).toBeTruthy();

    // A click on the visible count still toggles the section.
    fireEvent.click(countEl);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});
