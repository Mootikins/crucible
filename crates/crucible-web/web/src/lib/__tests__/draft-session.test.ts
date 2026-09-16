// New Session is a PROJECT action: the sessions tree offers it per project
// row, so the draft surface must open already aimed at that project.
import { describe, it, expect, beforeEach } from 'vitest';
import { produce } from 'solid-js/store';
import { windowStore, setStore } from '@/stores/windowStore';
import type { LayoutNode, TabGroup } from '@/types/windowTypes';
import { openDraftSession } from '../draft-session';

function resetLayout() {
  setStore(
    produce((s) => {
      s.layout = { id: 'pane-1', type: 'pane', tabGroupId: 'g-editor' } as LayoutNode;
      s.tabGroups = {
        'g-editor': { id: 'g-editor', tabs: [], activeTabId: null },
        'g-right': { id: 'g-right', tabs: [], activeTabId: null },
      } as Record<string, TabGroup>;
      s.activePaneId = null;
      s.edgePanels.right.layout = { id: 'right-pane', type: 'pane', tabGroupId: 'g-right' };
      s.edgePanels.right.mode = 'strip';
    }),
  );
}

/** Every draft tab in the shell, wherever it docked. */
const draftTabs = () =>
  Object.values(windowStore.tabGroups).flatMap((g) =>
    g.tabs.filter((t) => t.contentType === 'chat-draft'),
  );

describe('openDraftSession', () => {
  beforeEach(resetLayout);

  it('opens a draft with no project when none is named', () => {
    openDraftSession();
    expect(draftTabs()).toHaveLength(1);
    // Absent, not '': the composer reads an empty string as an explicit pick
    // of "no project", and this is "unset — you choose".
    expect(draftTabs()[0].metadata?.workspace).toBeUndefined();
  });

  it('carries the project the caller named', () => {
    openDraftSession({ workspace: '/home/me/crucible' });
    expect(draftTabs()[0].metadata?.workspace).toBe('/home/me/crucible');
  });

  it('retargets the open draft instead of opening a second', () => {
    openDraftSession({ workspace: '/home/me/crucible' });
    openDraftSession({ workspace: '/home/me/atlas' });
    // One draft surface at a time. Focusing the old draft unchanged would
    // silently discard the project the user just picked.
    expect(draftTabs()).toHaveLength(1);
    expect(draftTabs()[0].metadata?.workspace).toBe('/home/me/atlas');
  });

  it('keeps the draft id when it retargets', () => {
    openDraftSession({ workspace: '/home/me/crucible' });
    const id = draftTabs()[0].id;
    openDraftSession({ workspace: '/home/me/atlas' });
    // The tab handle the composer closes itself by must survive a retarget.
    expect(draftTabs()[0].metadata?.draftTabId).toBe(id);
  });

  it('can retarget a draft that opened with no project', () => {
    openDraftSession();
    openDraftSession({ workspace: '/home/me/crucible' });
    // The metadata key must EXIST from the first mount: panel props are keyed
    // from what is present then, so a key omitted at creation gets no reactive
    // channel and no later write can reach the composer.
    expect(draftTabs()).toHaveLength(1);
    expect(draftTabs()[0].metadata?.workspace).toBe('/home/me/crucible');
  });

  it("can retarget a draft the ribbon opened with an explicit 'no project'", () => {
    // The ribbon's own path. `''` is falsy, so a truthiness-guarded spread
    // dropped the key here while the retarget path tested presence — two rules
    // for one value, and the common entry point got the broken one.
    openDraftSession({ workspace: '' });
    expect('workspace' in (draftTabs()[0].metadata ?? {})).toBe(true);

    openDraftSession({ workspace: '/home/me/atlas' });
    expect(draftTabs()[0].metadata?.workspace).toBe('/home/me/atlas');
  });

  it('leaves an aimed draft alone when the caller names no project', () => {
    openDraftSession({ workspace: '/home/me/crucible' });
    openDraftSession();
    expect(draftTabs()[0].metadata?.workspace).toBe('/home/me/crucible');
  });
});
