import { Component, For, createSignal, onCleanup, onMount } from 'solid-js';
import {
  TreeView,
  useTreeView,
  type TreeCollection,
  type UseTreeViewReturn,
  type TreeViewLoadChildrenDetails,
  type TreeViewLoadChildrenCompleteDetails,
  type TreeViewRenameCompleteDetails,
  type TreeViewExpandedChangeDetails,
  type TreeViewSelectionChangeDetails,
} from '@ark-ui/solid';
import type { FileTreeNode as Node } from '@/lib/file-tree/types';
import type { TreeRootKind } from '@/lib/tree-root';
import { FileTreeNode, type FileTreeDnd } from './FileTreeNode';
import { attachFileDropTarget, canDropIntoFolder } from '@/lib/file-dnd';
import { shouldUseNativeMenu } from '@/lib/context-menu';
import { FileTreeContextMenu, itemsForNode, type ContextAction } from './FileTreeContextMenu';

/**
 * The two zag parts that render a clickable row (leaf / folder header). Both
 * carry `data-value` — the node's relPath — so the row under a right-click is
 * resolvable from the event target.
 */
const ROW_PARTS =
  '[data-scope="tree-view"][data-part="item"], [data-scope="tree-view"][data-part="branch-control"]';

/** DOM-id-safe encoding of a relPath (slashes/dots are awkward in ids/selectors). */
export const cssId = (value: string): string =>
  value ? value.replace(/[^a-zA-Z0-9_-]/g, '_') : 'root';

export interface FileTreeViewProps {
  collection: TreeCollection<Node>;
  rootKind: TreeRootKind;
  openFilePath: string | null;
  defaultExpandedValue?: string[];
  /** Project lazy loader; `undefined` for kilns (whole tree pre-built). */
  loadChildren?: (details: TreeViewLoadChildrenDetails<Node>) => Promise<Node[]>;
  /**
   * REQUIRED with `loadChildren`: the collection is controlled, so the machine
   * hands back the merged tree here and the owner must persist it — without
   * this, lazily loaded children are silently discarded (branch expands
   * empty).
   */
  onLoadedTree?: (rootNode: Node) => void;
  /** Opening a leaf routes through selection (one path for mouse AND keyboard). */
  onOpenLeaf: (node: Node) => void;
  onExpandedChange?: (expandedValue: string[]) => void;
  onContextAction: (action: ContextAction, node: Node) => void;
  /** Hand the live machine api to the parent (toolbar: collapse-all, reveal). */
  apiRef?: (api: UseTreeViewReturn<Node>) => void;
  /** Drag-and-drop wiring (absent → tree is drag-inert, e.g. in tests). */
  dnd?: FileTreeDnd;
  /** Live dotfile-visibility state (context-menu label). */
  showHidden?: boolean;
  /** Display-name transform (hidden-extension stripping). */
  formatName?: (name: string) => string;
  /**
   * Inline-rename commit (absent → F2/rename disabled). `relPath` is the
   * node's CURRENT identity; `newLabel` is the typed name (a single path
   * segment — the machine's input, not a path).
   */
  onRenameNode?: (relPath: string, newLabel: string) => void;
}

/**
 * `TreeView` shell. Selection is single; opening a leaf is routed exclusively
 * through `onSelectionChange` so mouse and keyboard share one code path (avoids
 * the routing-seam duplicate-side-effect bug class). Branches never "open" a
 * file — they expand (via `expandOnClick`). Rename (F2 or the context menu) commits
 * through `onRenameNode`.
 *
 * The context menu is ONE menu for the whole tree, hoisted here; the row it
 * acts on is resolved from the event target when it opens.
 */
export const FileTreeView: Component<FileTreeViewProps> = (props) => {
  const handleSelection = (d: TreeViewSelectionChangeDetails<Node>) => {
    const node = d.selectedNodes[0];
    if (node && !node.isDir) props.onOpenLeaf(node);
  };

  const api = useTreeView<Node>(() => ({
    collection: props.collection,
    selectionMode: 'single',
    expandOnClick: true,
    typeahead: true,
    defaultExpandedValue: props.defaultExpandedValue,
    loadChildren: props.loadChildren,
    onLoadChildrenComplete: (d: TreeViewLoadChildrenCompleteDetails<Node>) =>
      props.onLoadedTree?.(d.collection.rootNode),
    canRename: () => props.onRenameNode !== undefined,
    onRenameComplete: (d: TreeViewRenameCompleteDetails) =>
      props.onRenameNode?.(d.value, d.label),
    ids: { node: (v: string) => `filetree-node-${cssId(v)}` },
    onSelectionChange: handleSelection,
    onExpandedChange: (d: TreeViewExpandedChangeDetails<Node>) =>
      props.onExpandedChange?.(d.expandedValue),
  }));

  onMount(() => props.apiRef?.(api));

  // The tree surface itself is a move-to-root target (dropping on empty space
  // below the rows). Folder rows sit above it in the target stack, so the
  // innermost-zone protocol in file-dnd keeps this from stealing their drops.
  const [rootDropOver, setRootDropOver] = createSignal(false);
  const attachRootDrop = (el: HTMLElement) => {
    const dnd = props.dnd;
    if (!dnd) return;
    const cleanup = attachFileDropTarget(el, {
      zone: 'tree-root',
      canDrop: (source) => canDropIntoFolder(source, { rootId: dnd.rootId, relPath: '' }),
      onDragEnter: () => setRootDropOver(true),
      onDragLeave: () => setRootDropOver(false),
      onDrop: (source) => {
        setRootDropOver(false);
        dnd.onMove(source, '');
      },
    });
    onCleanup(cleanup);
  };

  // The tree owns ONE context menu; this is the row it acts on.
  const [menuNode, setMenuNode] = createSignal<Node | null>(null);

  /** The row under an event target, or `null` (empty space below the rows). */
  const rowNodeFor = (target: EventTarget | null): Node | null => {
    if (!(target instanceof Element)) return null;
    const value = target.closest(ROW_PARTS)?.getAttribute('data-value');
    return value == null ? null : (api().collection.findNode(value) ?? null);
  };

  /** True when the hoisted trigger may open on this event (and it now knows the row). */
  const routeToRow = (e: Event): boolean => {
    const node = rowNodeFor(e.target);
    if (!node || itemsForNode(node, props.rootKind).length === 0) return false;
    setMenuNode(node);
    return true;
  };

  /**
   * Capture-phase router for the single hoisted context trigger, which listens
   * on the wrapper OUTSIDE the tree — so this always runs first and can both
   * resolve the row and veto the open by stopping propagation. Vetoed:
   * Shift/images/links (the shared native-menu rule) and anything that hits no
   * row, which under the old per-row triggers simply had no trigger to hit.
   * Non-mouse pointerdown is zag's long-press path, and needs the same routing.
   */
  const attachContextRouter = (el: HTMLElement) => {
    const onContextMenu = (e: MouseEvent) => {
      if (shouldUseNativeMenu(e) || !routeToRow(e)) e.stopPropagation();
    };
    const onPointerDown = (e: PointerEvent) => {
      if (e.pointerType !== 'mouse' && !routeToRow(e)) e.stopPropagation();
    };
    el.addEventListener('contextmenu', onContextMenu, { capture: true });
    el.addEventListener('pointerdown', onPointerDown, { capture: true });
    // Not a leak either way — these live on the element and die with it — but
    // the sibling ref helper above cleans up, and one of two neighbours doing it
    // reads as an oversight in whichever one you notice second.
    onCleanup(() => {
      el.removeEventListener('contextmenu', onContextMenu, { capture: true });
      el.removeEventListener('pointerdown', onPointerDown, { capture: true });
    });
  };

  return (
    // lazyMount + unmountOnExit: without them ark-ui only sets `hidden` on
    // branch content, so every branch ever expanded stays mounted for the
    // session and the machine re-runs prop getters for all of it on every
    // select/expand. Mounted-row count is the cost lever, so collapsed
    // subtrees must actually leave the DOM. Safe with `loadChildren`: zag
    // keeps `loadingStatus` in machine context and short-circuits expansion
    // for already-loaded branches, so unmounting never refetches.
    <TreeView.RootProvider value={api} lazyMount unmountOnExit>
      <FileTreeContextMenu
        node={menuNode()}
        rootKind={props.rootKind}
        onAction={props.onContextAction}
        showHidden={props.showHidden}
      >
        <TreeView.Tree
          aria-label="File tree"
          ref={(el: HTMLElement) => {
            attachRootDrop(el);
            attachContextRouter(el);
          }}
          data-file-drop={rootDropOver() ? 'true' : undefined}
          class="px-1 min-h-full data-[file-drop=true]:bg-primary/5"
        >
          <For each={api().collection.rootNode.children}>
            {(node, i) => (
              <FileTreeNode
                node={node}
                indexPath={[i()]}
                openFilePath={props.openFilePath}
                formatName={props.formatName}
                dnd={props.dnd}
              />
            )}
          </For>
        </TreeView.Tree>
      </FileTreeContextMenu>
    </TreeView.RootProvider>
  );
};
