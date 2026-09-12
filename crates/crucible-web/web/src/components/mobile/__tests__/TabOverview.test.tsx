import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { TabOverview } from '@/components/mobile/TabOverview';
import type { Tab } from '@/types/windowTypes';

const tabs: Tab[] = [
  { id: 'a', title: 'Note A', contentType: 'file', metadata: {} },
  { id: 'b', title: 'Chat', contentType: 'chat', isModified: true, metadata: {} },
];

describe('TabOverview', () => {
  it('lists every open tab, most recent first', () => {
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={() => {}} onClose={() => {}} />);
    const names = screen.getAllByRole('listitem').map((li) => li.textContent);
    expect(names[0]).toContain('Chat');
    expect(names[1]).toContain('Note A');
  });

  it('marks the tab holding unsaved work', () => {
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={() => {}} onClose={() => {}} />);
    expect(screen.getByLabelText('Chat has unsaved changes')).toBeTruthy();
  });

  it('picks a tab', () => {
    const onPick = vi.fn();
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={onPick} onClose={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Note A' }));
    expect(onPick).toHaveBeenCalledWith('a');
  });

  it('closes a tab without picking it', () => {
    const onPick = vi.fn();
    const onClose = vi.fn();
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={onPick} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Close Note A' }));
    expect(onClose).toHaveBeenCalledWith('a');
    expect(onPick).not.toHaveBeenCalled();
  });

  // A phone cannot use window.confirm reliably — an installed PWA may suppress
  // it — so the card asks in place before it discards anything.
  it('asks before closing a tab with unsaved work', () => {
    const onClose = vi.fn();
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={() => {}} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Close Chat' }));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText('Discard unsaved changes?')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(onClose).toHaveBeenCalledWith('b');
  });

  it('keeps a tab whose close was asked about and declined', () => {
    const onClose = vi.fn();
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={() => {}} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Close Chat' }));
    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByText('Discard unsaved changes?')).toBeNull();
  });

  it('closes a clean tab without asking', () => {
    const onClose = vi.fn();
    render(() => <TabOverview tabs={tabs} activeId="a" onPick={() => {}} onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Close Note A' }));
    expect(onClose).toHaveBeenCalledWith('a');
  });

  it('says so when nothing is open', () => {
    render(() => <TabOverview tabs={[]} activeId={null} onPick={() => {}} onClose={() => {}} />);
    expect(screen.getByText('No tabs are open.')).toBeTruthy();
  });
});
