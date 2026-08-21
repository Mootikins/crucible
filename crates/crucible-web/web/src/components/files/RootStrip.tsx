import { Component, For, Show } from 'solid-js';
import type { RosterGroup, TreeRoot } from '@/lib/tree-root';
import { rootKey } from '@/lib/tree-root';
import type { SessionRoot } from '@/lib/session-roots';
import { RootDropdown } from './RootDropdown';
import { Link2 } from '@/lib/icons';

/**
 * The file tree's root picker: a strip of the SESSION's own roots, plus the
 * full roster behind a chevron.
 *
 * A session reaches its workspace and its attached kilns — usually one to
 * three roots — so those are one click away rather than a menu round-trip.
 * The registry can hold many more, and those stay behind the chevron, which
 * is the existing `RootDropdown` unchanged: branches, worktree creation and
 * clone all still live there.
 *
 * A kiln the session is NOT attached to renders dimmed, because browsing is
 * not attaching. Picking one only re-roots the tree; the agent still cannot
 * read or cite it. Attaching is a separate, explicit click — a navigation
 * gesture must never widen what the agent can see.
 */
export const RootStrip: Component<{
  /** The session's own roots, plus any unattached kiln being browsed. */
  roots: SessionRoot[];
  active: SessionRoot | null;
  onSelect: (r: TreeRoot) => void;
  /** Attach the browsed kiln to the session (`session.connect_kiln`). */
  onAttach: (r: SessionRoot) => void;
  /** Full roster for the overflow chevron. */
  groups: RosterGroup[];
  onNotice?: (msg: string | null) => void;
}> = (props) => {
  const isActive = (r: SessionRoot) =>
    !!props.active && rootKey(props.active) === rootKey(r);

  return (
    <div class="flex items-center gap-0.5 min-w-0 flex-1" data-testid="root-strip">
      <div class="flex items-center gap-0.5 min-w-0 overflow-x-auto scrollbar-none">
        <For each={props.roots}>
          {(r) => (
            <button
              type="button"
              data-testid={`root-tab-${rootKey(r)}`}
              data-origin={r.origin}
              aria-pressed={isActive(r)}
              title={
                r.origin === 'other-kiln'
                  ? `${r.name} — browsing only; this session is not attached`
                  : r.path
              }
              onClick={() => props.onSelect(r)}
              classList={{
                'px-2 py-1 rounded text-xs whitespace-nowrap transition-colors': true,
                'bg-surface-elevated text-shell-ink': isActive(r),
                'text-muted hover:text-shell-ink hover:bg-hover-wash': !isActive(r),
                // Dimmed + italic: the tree shows it, the agent does not.
                'italic opacity-60': r.origin === 'other-kiln',
              }}
            >
              {r.name}
            </button>
          )}
        </For>
      </div>

      {/* Only on the ACTIVE unattached kiln: an affordance for a root you are
          not looking at would be a claim about a corpus you cannot see. */}
      <Show when={props.active?.origin === 'other-kiln' ? props.active : null} keyed>
        {(root) => (
          <button
            type="button"
            data-testid="root-attach"
            title={`Let this session query ${root.name}`}
            onClick={() => props.onAttach(root)}
            class="flex items-center gap-1 px-1.5 py-1 rounded text-[11px] text-muted hover:text-shell-ink hover:bg-hover-wash whitespace-nowrap transition-colors"
          >
            <Link2 class="w-3 h-3" /> Attach
          </button>
        )}
      </Show>

      <RootDropdown
        groups={props.groups}
        selectedKey={null}
        onSelect={props.onSelect}
        activeRoot={props.active}
        onNotice={props.onNotice}
        bare
      />
    </div>
  );
};
