import { Component, Show, createEffect } from 'solid-js';
import { Key } from '@solid-primitives/keyed';
import type { RosterGroup, TreeRoot } from '@/lib/tree-root';
import { rootKey, rosterIndex } from '@/lib/tree-root';

/**
 * Native grouped `<select>` for choosing the browsable root (matches the
 * existing `KilnSelector` idiom). Empty groups are omitted; an empty roster
 * renders a "No roots" fallback and no `<select>`.
 */
export const RootDropdown: Component<{
  groups: RosterGroup[];
  selectedKey: string | null;
  onSelect: (r: TreeRoot) => void;
}> = (props) => {
  const index = () => rosterIndex(props.groups);
  const nonEmpty = () => props.groups.filter((g) => g.roots.length > 0);

  let ref: HTMLSelectElement | undefined;
  // Re-apply the DOM value whenever the ROSTER changes, not just the key:
  // assigning select.value before the matching <option> exists (kilns load
  // async, after projects) silently resets the browser to option 0, and the
  // value expression never re-runs when the option finally lands — the
  // select then DISPLAYS the wrong root while the tree shows the right one.
  // User effects run after Solid's render effects, so the options are in
  // the DOM by the time this assigns.
  createEffect(() => {
    props.groups;
    const v = props.selectedKey ?? '';
    if (ref && ref.value !== v) ref.value = v;
  });

  return (
    <Show
      when={nonEmpty().length > 0}
      fallback={<span class="text-xs text-muted-dark">No roots</span>}
    >
      <select
        ref={ref}
        data-testid="root-dropdown"
        aria-label="Browse root"
        value={props.selectedKey ?? ''}
        onChange={(e) => {
          const r = index().get(e.currentTarget.value);
          if (r) props.onSelect(r);
        }}
        class="max-w-[12rem] bg-surface-elevated text-shell-ink text-xs px-2 py-1 rounded border border-hairline focus:border-primary focus:outline-none"
      >
        {/* Keyed by stable identity, NOT reference: buildRoster returns all-new
            objects every rebuild, and reference-keyed rows would REMOVE and
            re-add every <option> — removing the selected option makes the
            browser silently reset the select to index 0 (it raced and beat
            every re-assignment we tried; jsdom doesn't emulate the reset, so
            only real browsers ever saw it). Stable keys update rows in place
            and the selected option never leaves the DOM. */}
        <Key each={nonEmpty()} by={(g) => g.label}>
          {(g) => (
            <optgroup label={g().label}>
              <Key each={g().roots} by={rootKey}>
                {(r) => (
                  <option value={rootKey(r())} title={r().path}>
                    {r().name}
                  </option>
                )}
              </Key>
            </optgroup>
          )}
        </Key>
      </select>
    </Show>
  );
};
