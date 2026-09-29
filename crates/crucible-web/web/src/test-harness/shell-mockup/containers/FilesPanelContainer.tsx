/**
 * The files rail on the mock store and the window store. A port reads
 * `useKilns`, `useProjects` and `useListDir`, and the review counts from
 * `reviewStore.hunksForPath`.
 */
import type { Component } from 'solid-js';
import { windowActions, windowStore } from '@/windowing/store';
import { openNote, whereFor } from '../actions';
import { KILN_PATHS, PROJECT_PATHS } from '../data';
import { FileTree } from '../components/files/FileTree';
import { buildTree } from '../components/files/tree';
import { TOUCHED, pendingHunks, state } from '../state';

const KILN_TREE = buildTree(KILN_PATHS);

/** The path of the note in the focused tab, if that tab shows a note. */
function activePath(): string | undefined {
  const id = windowStore.activePaneId ? windowActions.getPaneTabGroupId(windowStore.activePaneId) : null;
  const g = id ? windowStore.tabGroups[id] : undefined;
  return g?.tabs.find((t) => t.id === g.activeTabId)?.metadata?.path as string | undefined;
}

export const FilesPanelContainer: Component = () => {
  const session = () => state.sessions[state.active]!;
  return (
    <FileTree
      kiln={KILN_TREE}
      projectEntries={PROJECT_PATHS}
      attached={(root) => session().roots.includes(root)}
      color={session().color}
      activePath={activePath()}
      pendingFor={(path) => pendingHunks(state.active).filter((id) => state.hunks[id]!.path === path).length}
      touched={(path) => (TOUCHED[state.active] ?? []).includes(path)}
      onOpen={(path, e) => openNote(path, { where: whereFor(e) })}
    />
  );
};
