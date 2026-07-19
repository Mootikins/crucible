import { Component, For, Show } from 'solid-js';
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

  return (
    <Show
      when={nonEmpty().length > 0}
      fallback={<span class="text-xs text-muted-dark">No roots</span>}
    >
      <select
        data-testid="root-dropdown"
        aria-label="Browse root"
        value={props.selectedKey ?? ''}
        onChange={(e) => {
          const r = index().get(e.currentTarget.value);
          if (r) props.onSelect(r);
        }}
        class="max-w-[12rem] bg-surface-elevated text-shell-ink text-xs px-2 py-1 rounded border border-hairline focus:border-primary focus:outline-none"
      >
        <For each={nonEmpty()}>
          {(g) => (
            <optgroup label={g.label}>
              <For each={g.roots}>
                {(r) => (
                  <option value={rootKey(r)} title={r.path}>
                    {r.name}
                  </option>
                )}
              </For>
            </optgroup>
          )}
        </For>
      </select>
    </Show>
  );
};
