/**
 * Framework-specific binding context for FlexLayout.
 *
 * Contains operations that require a UI framework: factory mounting,
 * tab/tabset customization (render values contain framework elements),
 * popup/context menu rendering, and floating panel lifecycle callbacks.
 *
 * Implementations will import their framework (SolidJS, React, etc.)
 * and provide concrete element types for the generic parameter.
 *
 * Split from the original ILayoutContext. See {@link ICoreRenderContext}
 * for the framework-agnostic counterpart.
 */

import type { TabNode } from "../model/TabNode";
import type { TabSetNode } from "../model/TabSetNode";
import type { BorderNode } from "../model/BorderNode";

/**
 * Tab render values passed to customization callbacks.
 * Generic over the element type produced by the UI framework.
 */
export interface ITabRenderValues<E = unknown> {
    leading: E | undefined;
    content: E | undefined;
    buttons: E[];
}

/**
 * TabSet render values passed to customization callbacks.
 * Generic over the element type produced by the UI framework.
 */
export interface ITabSetRenderValues<E = unknown> {
    leading: E | undefined;
    stickyButtons: E[];
    buttons: E[];
    overflowPosition: number | undefined;
}

/** Popup menu item descriptor. */
export interface IPopupItem {
    index: number;
    node: TabNode;
}

/** Context menu item descriptor. */
export interface IContextMenuItem {
    label: string;
    action: () => void;
}

export interface IBindingContext<E = unknown> {
    /** Create a framework element for a tab node's content. */
    factory(node: TabNode): E;

    /** Customize a tab's rendering (leading content, buttons, etc.). */
    customizeTab(tabNode: TabNode, renderValues: ITabRenderValues<E>): void;

    /** Customize a tabset's rendering (buttons, sticky buttons, overflow position). */
    customizeTabSet(
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues<E>,
    ): void;

    /** Show a popup menu anchored to a trigger element. */
    showPopup(
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: IPopupItem[],
        onSelect: (item: IPopupItem) => void,
    ): void;

    /** Show a context menu at the mouse event position. */
    showContextMenu(
        event: MouseEvent,
        items: IContextMenuItem[],
    ): void;

    /** Called when a floating panel drag starts. Set by FloatingPanel component. */
    onFloatDragStart?: (e: PointerEvent) => void;

    /** Called when a floating panel should dock back. Set by FloatingPanel component. */
    onFloatDock?: () => void;

    /** Called when a floating panel should close. Set by FloatingPanel component. */
    onFloatClose?: () => void;
}
