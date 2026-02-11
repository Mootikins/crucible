import {
  type Component,
  createSignal,
  onMount,
  onCleanup,
  For,
} from "solid-js";
import { Portal } from "solid-js/web";
import { CLASSES } from "../../flexlayout/core/Types";

export interface PopupMenuItem {
  label: string;
  onSelect: () => void;
  selected?: boolean;
}

export interface PopupPosition {
  left?: string;
  right?: string;
  top?: string;
  bottom?: string;
}

export interface PopupProps {
  position: PopupPosition;
  items: PopupMenuItem[];
  onClose: () => void;
  classNameMapper?: (defaultClassName: string) => string;
}

export function clampIndex(index: number, max: number): number {
  if (max <= 0) return -1;
  if (index < 0) return max - 1;
  if (index >= max) return 0;
  return index;
}

export function buildPopupItemClass(
  isSelected: boolean,
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  let cls = map(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM);
  if (isSelected) {
    cls += ` ${map(CLASSES.FLEXLAYOUT__POPUP_MENU_ITEM__SELECTED)}`;
  }
  return cls;
}

export function buildPopupContainerClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER);
}

export function buildPopupMenuClass(
  mapper?: (cls: string) => string,
): string {
  const map = (c: string) => (mapper ? mapper(c) : c);
  return map(CLASSES.FLEXLAYOUT__POPUP_MENU);
}

export function buildPopupStyle(
  position: PopupPosition,
): Record<string, string> {
  const style: Record<string, string> = {
    position: "absolute",
    "z-index": "1002",
  };
  if (position.left) style.left = position.left;
  if (position.right) style.right = position.right;
  if (position.top) style.top = position.top;
  if (position.bottom) style.bottom = position.bottom;
  return style;
}

export const Popup: Component<PopupProps> = (props) => {
  const map = (cls: string) => props.classNameMapper?.(cls) ?? cls;

  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  let menuRef: HTMLDivElement | undefined;

  const initialIndex = props.items.findIndex((item) => item.selected);

  onMount(() => {
    if (initialIndex >= 0) {
      setFocusedIndex(initialIndex);
    }

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
        setFocusedIndex((prev) => clampIndex(prev + 1, count));
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        setFocusedIndex((prev) => clampIndex(prev - 1, count));
        break;
      }
      case "Enter": {
        e.preventDefault();
        const idx = focusedIndex();
        if (idx >= 0 && idx < count) {
          props.items[idx].onSelect();
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

  const handleItemClick = (item: PopupMenuItem, e: MouseEvent) => {
    e.stopPropagation();
    item.onSelect();
    props.onClose();
  };

  return (
    <Portal>
      <div
        class={map(CLASSES.FLEXLAYOUT__POPUP_MENU_CONTAINER)}
        data-layout-path="/popup-menu-container"
        style={buildPopupStyle(props.position)}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <div
          ref={menuRef}
          class={map(CLASSES.FLEXLAYOUT__POPUP_MENU)}
          data-layout-path="/popup-menu"
          tabIndex={0}
          onKeyDown={handleKeyDown}
        >
          <For each={props.items}>
            {(item, index) => (
              <div
                class={buildPopupItemClass(
                  index() === focusedIndex() || (item.selected === true && focusedIndex() === -1),
                  props.classNameMapper,
                )}
                data-layout-path={`/popup-menu/tb${index()}`}
                onClick={(e) => handleItemClick(item, e)}
                onPointerEnter={() => setFocusedIndex(index())}
              >
                {item.label}
              </div>
            )}
          </For>
        </div>
      </div>
    </Portal>
  );
};
