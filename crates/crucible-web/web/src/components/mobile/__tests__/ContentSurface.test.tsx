import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { createSignal, onMount } from 'solid-js';
import { ContentSurface } from '@/components/mobile/ContentSurface';
import { getGlobalRegistry, resetGlobalRegistry } from '@/lib/panel-registry';
import type { Tab } from '@/types/windowTypes';

let mounts = 0;

/** A registered panel that counts its mounts and shows the prop it received. */
function CountingPanel(props: { label?: string }) {
  onMount(() => {
    mounts += 1;
  });
  return <div data-testid="counting-panel">{props.label}</div>;
}

const noteTab = (over: Partial<Tab> = {}): Tab => ({
  id: 'tab-1',
  title: 'Note',
  contentType: 'file',
  metadata: { label: 'first' },
  ...over,
});

beforeEach(() => {
  mounts = 0;
  resetGlobalRegistry();
  getGlobalRegistry().register('file', 'File', CountingPanel, 'center');
});

describe('ContentSurface', () => {
  it('draws the registered panel for the tab, with its metadata as props', () => {
    const [tab] = createSignal<Tab | null>(noteTab());
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    expect(screen.getByTestId('counting-panel').textContent).toBe('first');
  });

  it('draws the empty state when there is no tab', () => {
    const [tab] = createSignal<Tab | null>(null);
    render(() => <ContentSurface tab={tab} empty={<p>nothing open</p>} />);
    expect(screen.getByText('nothing open')).toBeTruthy();
  });

  it('says so when a tab names a type the registry does not know', () => {
    const [tab] = createSignal<Tab | null>(noteTab({ contentType: 'canvas' }));
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    expect(screen.getByText('Unknown content type')).toBeTruthy();
  });

  // The editor writes isModified back on each keystroke. If that write
  // remounted the panel, every keystroke would discard the buffer.
  it('keeps the panel mounted when a field other than identity changes', () => {
    const [tab, setTab] = createSignal<Tab | null>(noteTab());
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    setTab(noteTab({ isModified: true }));
    setTab(noteTab({ isModified: false, title: 'Renamed' }));
    expect(mounts).toBe(1);
  });

  it('delivers a later metadata write without a remount', () => {
    const [tab, setTab] = createSignal<Tab | null>(noteTab());
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    setTab(noteTab({ metadata: { label: 'second' } }));
    expect(screen.getByTestId('counting-panel').textContent).toBe('second');
    expect(mounts).toBe(1);
  });

  it('mounts a new panel when the tab identity changes', () => {
    const [tab, setTab] = createSignal<Tab | null>(noteTab());
    render(() => <ContentSurface tab={tab} empty={<p>empty</p>} />);
    setTab(noteTab({ id: 'tab-2', metadata: { label: 'other' } }));
    expect(mounts).toBe(2);
    expect(screen.getByTestId('counting-panel').textContent).toBe('other');
  });
});
