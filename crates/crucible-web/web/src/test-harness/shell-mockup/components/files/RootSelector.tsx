/**
 * The root switcher at the top left of the Files pane, as Obsidian's vault
 * switcher: an up and down chevron and the root name, flat, with no box.
 * Its menu lists the kilns and the projects, and a way to manage them. A
 * root of the active session shows a bar in the session colour.
 */
import type { Component } from 'solid-js';
import { Show } from 'solid-js';
import { ChevronsUpDown } from 'lucide-solid';
import { Menu, type MenuEntry } from '../primitives/Menu';
import type { RootKey } from './FileTree';

/** One root in the menu. The real app reads `useKilns` and `useProjects`. */
export interface RootOptionView {
  key: RootKey;
  label: string;
  /** What the root is: "kiln", "project" or "workspace". */
  kind: string;
  /** Is this root one of the active session's roots? */
  attached: boolean;
}

export interface RootSelectorProps {
  roots: readonly RootOptionView[];
  current: RootOptionView;
  /** The active session's identity colour. */
  color: string;
  onPick: (key: RootKey) => void;
  onManage: () => void;
}

export const RootSelector: Component<RootSelectorProps> = (props) => {
  const entries = (): MenuEntry[] => [
    ...props.roots.map((r): MenuEntry => ({
      kind: 'item',
      label: r.label,
      hint: r.kind,
      checked: r.key === props.current.key,
      bar: r.attached ? props.color : undefined,
      onSelect: () => props.onPick(r.key),
    })),
    { kind: 'sep' },
    { kind: 'item', label: 'Manage projects and kilns…', onSelect: props.onManage },
  ];
  return (
    <Menu
      label="Switch the root"
      triggerClass={`mk-rootsel${props.current.attached ? '' : ' dim'}`}
      trigger={
        <>
          <ChevronsUpDown class="mk-i" />
          <span class="mk-t">{props.current.label}</span>
          <Show when={props.current.attached}>
            <span class="mk-sessionbar" style={{ background: props.color }} title="A root of the active session" />
          </Show>
        </>
      }
      entries={entries()}
    />
  );
};
