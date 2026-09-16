import { describe, it, expect, beforeEach } from 'vitest';
import { createComputed, onCleanup, type Component } from 'solid-js';
import { render } from '@solidjs/testing-library';
import { produce } from 'solid-js/store';
import { Pane } from '../Pane';
import { windowStore, windowActions, setStore } from '@/windowing/store';
import { findFirstPane, generateId } from '@/windowing/model/tree';
import type { WindowingSlots, WindowingContextValue } from '@/windowing/components/context';
import { CoreProviders, configureRails } from './fixtures';

// Renders Pane against the core store. An empty pane holds one quiet
// affordance: the state it is in, and the rows the app gives it. Drawing
// nothing at all read as a rendering failure on a wide screen.

let paneId: string;
let groupId: string;

beforeEach(() => {
  configureRails();
  const pane = findFirstPane(windowStore.layout)!;
  paneId = pane.id;
  groupId = pane.tabGroupId!;
});

const hints: WindowingSlots = {
  emptyPaneHints: () => [
    { label: 'First hint', chord: 'Ctrl+1' },
    { label: 'Second hint', chord: 'Ctrl+2' },
  ],
};

const renderPane = (id: () => string, props: Partial<WindowingContextValue> = { slots: hints }) =>
  render(() => (
    <CoreProviders {...props}>
      <Pane paneId={id()} />
    </CoreProviders>
  ));

describe('Pane — empty center', () => {
  it('renders the empty-pane affordance when the pane has no tabs', () => {
    const { getByTestId, container } = renderPane(() => paneId);

    // No tab strip — nothing to strip.
    expect(container.querySelector('[data-tab-id]')).toBeNull();

    // The pane says what it is, and prints the rows the app gave it.
    const affordance = getByTestId('empty-pane');
    expect(affordance.textContent).toContain('First hint');
    expect(affordance.textContent).toContain('Second hint');
    const chords = Array.from(affordance.querySelectorAll('kbd')).map((k) => k.textContent);
    expect(chords).toEqual(['Ctrl+1', 'Ctrl+2']);
  });

  it('prints no rows when the app gives no hints', () => {
    const { getByTestId } = renderPane(() => paneId, { slots: {} });
    expect(getByTestId('empty-pane').querySelectorAll('li')).toHaveLength(0);
  });

  it('names the region empty when no other pane holds a tab', () => {
    const { getByTestId } = renderPane(() => paneId);

    expect(getByTestId('empty-pane').getAttribute('data-empty-pane')).toBe('region');
    expect(getByTestId('empty-pane').textContent).toContain('Nothing open');
  });

  it('names the pane empty when a sibling pane still holds work', () => {
    const siblingPaneId = generateId();
    const siblingGroupId = generateId();
    setStore(
      produce((s) => {
        s.tabGroups[siblingGroupId] = {
          id: siblingGroupId,
          tabs: [{ id: 'sibling-tab', title: 'note.md', contentType: 'alpha' }],
          activeTabId: 'sibling-tab',
        };
        s.layout = {
          id: generateId(),
          type: 'split',
          direction: 'horizontal',
          splitRatio: 0.5,
          first: { id: paneId, type: 'pane', tabGroupId: groupId },
          second: { id: siblingPaneId, type: 'pane', tabGroupId: siblingGroupId },
        };
      }),
    );

    const { getByTestId } = renderPane(() => paneId);

    expect(getByTestId('empty-pane').getAttribute('data-empty-pane')).toBe('pane');
    expect(getByTestId('empty-pane').textContent).toContain('Empty pane');
  });

  it('leaves a rail pane alone — the affordance is the centre tiling only', () => {
    const railPane = findFirstPane(windowStore.edgePanels.left.layout)!;
    // Empty it, so the only reason it could render no affordance is its region.
    setStore(
      produce((s) => {
        s.tabGroups[railPane.tabGroupId!] = {
          id: railPane.tabGroupId!,
          tabs: [],
          activeTabId: null,
        };
      }),
    );
    const { queryByTestId } = renderPane(() => railPane.id);

    // A rail slot is a fixed tool stack; "open a note here" is not an
    // instruction it can honour.
    expect(queryByTestId('empty-pane')).toBeNull();
  });

  it('renders the tab bar and the tab body once a tab is added', () => {
    const { container, queryByTestId } = renderPane(() => paneId);

    expect(container.querySelector('[data-tab-id="note-tab"]')).toBeNull();

    windowActions.addTab(groupId, {
      id: 'note-tab',
      title: 'note.md',
      contentType: 'alpha',
    });

    expect(container.querySelector('[data-tab-id="note-tab"]')).toBeTruthy();
    expect(queryByTestId('content-alpha')?.textContent).toBe('note.md');
  });
});

describe('Pane — the renderer gets the LIVE tab', () => {
  /**
   * Retargeting an open draft rewrites metadata on the SAME tab, so the pane
   * must deliver that write to a body it has already mounted.
   *
   * The pane deliberately does not re-render on tab-object churn — that would
   * remount the body and discard in-progress edits — so the renderer gets an
   * accessor, and runs untracked. This asserts both halves: a frozen value,
   * or a remount.
   */
  let renders = 0;

  const Probe: Component<{ workspace?: unknown }> = (props) => {
    renders += 1;
    return (
      <div>
        <span data-testid="probe-workspace">{String(props.workspace ?? 'unset')}</span>
        <input data-testid="probe-input" />
      </div>
    );
  };

  // The renderer reads the tab while it renders, as an app renderer may. A
  // tracked read here would remount the body on every tab write.
  const probeRenderer: WindowingContextValue['renderContent'] = (tab) => {
    void tab().title;
    return <Probe workspace={tab().metadata?.workspace} />;
  };

  beforeEach(() => {
    renders = 0;
    windowActions.addTab(groupId, {
      id: 'tab-draft-1',
      title: 'New Session',
      contentType: 'draft',
      metadata: { workspace: '/home/me/crucible' },
    });
  });

  const renderProbe = () => renderPane(() => paneId, { renderContent: probeRenderer });

  it('passes the metadata a tab was created with', () => {
    const { getByTestId } = renderProbe();
    expect(getByTestId('probe-workspace').textContent).toBe('/home/me/crucible');
  });

  it('delivers a later write to the already-mounted body', () => {
    const { getByTestId } = renderProbe();

    windowActions.updateTab(groupId, 'tab-draft-1', {
      metadata: { workspace: '/home/me/atlas' },
    });

    expect(getByTestId('probe-workspace').textContent).toBe('/home/me/atlas');
  });

  it('does not remount the body to do it', () => {
    const { getByTestId } = renderProbe();
    const before = renders;
    (getByTestId('probe-input') as HTMLInputElement).value = 'unsent draft';

    windowActions.updateTab(groupId, 'tab-draft-1', {
      title: 'Renamed',
      metadata: { workspace: '/home/me/atlas' },
    });

    // A remount would discard whatever the user had typed.
    expect(renders).toBe(before);
    expect((getByTestId('probe-input') as HTMLInputElement).value).toBe('unsent draft');
  });
});

describe('Pane — each panel reads its own tab', () => {
  /**
   * A switch of the active tab replaces the panel. Until the pane disposes the
   * old panel, the old panel can still read its accessor. That accessor must
   * give the old tab, never the tab that the user selected. The cleanup of the
   * old panel runs after the store names the new tab.
   */
  it('never gives the first panel the metadata of the second tab', () => {
    const seen: Record<string, unknown[]> = {};
    const recorder: WindowingContextValue['renderContent'] = (tab) => {
      const id = tab().id;
      seen[id] = [];
      createComputed(() => seen[id].push(tab().metadata?.label));
      // A panel may read its tab when the pane disposes it, for example to
      // save a draft.
      onCleanup(() => seen[id].push(tab().metadata?.label));
      return <div data-testid={`body-${id}`} />;
    };
    windowActions.addTab(groupId, {
      id: 'first',
      title: 'First',
      contentType: 'alpha',
      metadata: { label: 'first-label' },
    });
    windowActions.addTab(groupId, {
      id: 'second',
      title: 'Second',
      contentType: 'alpha',
      metadata: { label: 'second-label' },
    });
    windowActions.setActiveTab(groupId, 'first');
    const { queryByTestId } = renderPane(() => paneId, { renderContent: recorder });
    expect(queryByTestId('body-first')).toBeTruthy();

    windowActions.setActiveTab(groupId, 'second');

    expect(queryByTestId('body-second')).toBeTruthy();
    expect(seen.first).not.toContain('second-label');
    expect(seen.second).toEqual(['second-label']);
  });
});
