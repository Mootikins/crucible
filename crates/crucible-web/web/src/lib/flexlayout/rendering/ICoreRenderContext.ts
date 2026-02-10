/**
 * Framework-agnostic rendering context for FlexLayout.
 *
 * Model access, action dispatch, DOM operations, layout geometry,
 * drag-and-drop, and edit state. Zero framework imports.
 *
 * Split from the original ILayoutContext. See {@link IBindingContext}
 * for the framework-specific counterpart.
 */

import type { Model } from "../model/Model";
import type { Node } from "../model/Node";
import type { TabNode } from "../model/TabNode";
import type { LayoutAction } from "../model/Action";
import type { Rect } from "../core/Rect";

export interface ICoreRenderContext {
    readonly model: Model;

    doAction(action: LayoutAction): Node | undefined;

    getClassName(defaultClassName: string): string;

    getRootDiv(): HTMLDivElement | undefined;
    getMainElement(): HTMLDivElement | undefined;
    /** Layout root element — used for appending drag outlines, computing relative positions. */
    getLayoutRootDiv(): HTMLDivElement | undefined;

    getDomRect(): Rect;
    getBoundingClientRect(div: HTMLElement): Rect;

    getWindowId(): string;

    isRealtimeResize(): boolean;

    redraw(): void;
    /** Incremented on each layout recalculation; child components track this for re-measure. */
    getRevision(): number;

    setDragNode(event: DragEvent, node: Node): void;
    clearDragMain(): void;

    setEditingTab(tab?: TabNode): void;
    getEditingTab(): TabNode | undefined;
}
