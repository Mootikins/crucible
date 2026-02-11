import {
  type Component,
  createSignal,
  onMount,
  onCleanup,
  For,
  Show,
} from "solid-js";
import { Portal } from "solid-js/web";
import { CLASSES } from "../../flexlayout/core/Types";

export interface ContextMenuItem {
  label: string;
  action: () => void;
}

export interface ContextMenuProps {
  position: { x: number; y: number };
  items: ContextMenuItem[];
  onClose: () => void;
  classNameMapper?: (defaultClassName: string) => string;
}

export function clampContextIndex(index: number, max: number): number {
  if (max <= 0) return -1;
  if (index < 0) return max - 1;
  if (index >= max) return 0;
  return index;
}

export function buildContextMenuItemClass(
  isFocused: boolean,
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  let cls = map(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
  if (isFocused) {
    cls += ` ${map(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED)}`;
  }
  return cls;
}

export function buildContextMenuStyle(
  position: { x: number; y: number },
): Record<string, string> {
  return {
    position: "absolute",
    left: `${position.x}px`,
    top: `${position.y}px`,
  };
}

export const ContextMenu: Component<ContextMenuProps> = (props) => {
  const map = (cls: string) => props.classNameMapper?.(cls) ?? cls;

  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  let menuRef: HTMLDivElement | undefined;

  onMount(() => {
    requestAnimationFrame(() => menuRef?.focus());

    const onPointerDown = (e: PointerEvent) => {
      if (menuRef && !menuRef.contains(e.target as Node)) {
        props.onClose();
      }
    };

    requestAnimationFrame(() => {
      document.addEventListener("pointerdown", onPointerDown);
    });

    onCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown);
    });
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    const count = props.items.length;
    if (count === 0) return;

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        setFocusedIndex((prev) => clampContextIndex(prev + 1, count));
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        setFocusedIndex((prev) => clampContextIndex(prev - 1, count));
        break;
      }
      case "Enter": {
        e.preventDefault();
        const idx = focusedIndex();
        if (idx >= 0 && idx < count) {
          props.items[idx].action();
          props.onClose();
        }
        break;
      }
      case "Escape": {
        e.preventDefault();
        props.onClose();
        break;
      }
    }
  };

  const handleItemClick = (item: ContextMenuItem, e: MouseEvent) => {
    e.stopPropagation();
    item.action();
    props.onClose();
  };

  return (
    <Portal>
      <div
        class={map(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
        data-layout-path="/context-menu-container"
        style={{ position: "absolute", inset: "0", "z-index": "1002" }}
        onPointerDown={() => props.onClose()}
      >
        <div
          ref={menuRef}
          class={map(CLASSES.FLEXLAYOUT__POPUP_MENU)}
          data-layout-path="/context-menu"
          tabIndex={0}
          style={buildContextMenuStyle(props.position)}
          onKeyDown={handleKeyDown}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <Show
            when={props.items.length > 0}
            fallback={
              <div
                class={map(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM)}
                style={{ opacity: "0.5", cursor: "default" }}
              >
                No actions available
              </div>
            }
          >
            <For each={props.items}>
              {(item, index) => (
                <div
                  class={buildContextMenuItemClass(
                    index() === focusedIndex(),
                    props.classNameMapper,
                  )}
                  data-context-menu-item=""
                  onClick={(e) => handleItemClick(item, e)}
                  onPointerEnter={() => setFocusedIndex(index())}
                >
                  {item.label}
                </div>
              )}
            </For>
          </Show>
        </div>
      </div>
    </Portal>
  );
};
