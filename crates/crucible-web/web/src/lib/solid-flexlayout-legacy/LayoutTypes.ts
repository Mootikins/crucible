import { JSX } from "solid-js";
import { Rect } from "../flexlayout/core/Rect";
import { BorderNode } from "../flexlayout/model/BorderNode";
import { Model } from "../flexlayout/model/Model";
import { Node } from "../flexlayout/model/Node";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";

export interface ILayoutProps {
    model: Model;
    factory: (node: TabNode) => JSX.Element;
    onAction?: (action: any) => any | undefined;
    onModelChange?: (model: Model, action: any) => void;
    onRenderTab?: (node: TabNode, renderValues: ITabRenderValues) => void;
    onRenderTabSet?: (
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues,
    ) => void;
    classNameMapper?: (defaultClassName: string) => string;
}

export interface ITabSetRenderValues {
    leading: JSX.Element | undefined;
    stickyButtons: JSX.Element[];
    buttons: JSX.Element[];
    overflowPosition: number | undefined;
}

export interface ITabRenderValues {
    leading: JSX.Element | undefined;
    content: JSX.Element | undefined;
    buttons: JSX.Element[];
}

export interface ILayoutContext {
    model: Model;
    factory: (node: TabNode) => JSX.Element;
    getClassName: (defaultClassName: string) => string;
    doAction: (action: any) => Node | undefined;
    customizeTab: (tabNode: TabNode, renderValues: ITabRenderValues) => void;
    customizeTabSet: (
        tabSetNode: TabSetNode | BorderNode,
        renderValues: ITabSetRenderValues,
    ) => void;
    getRootDiv: () => HTMLDivElement | undefined;
    getMainElement: () => HTMLDivElement | undefined;
    getDomRect: () => Rect;
    getBoundingClientRect: (div: HTMLElement) => Rect;
    getWindowId: () => string;
    setEditingTab: (tab?: TabNode) => void;
    getEditingTab: () => TabNode | undefined;
    isRealtimeResize: () => boolean;
    getLayoutRootDiv: () => HTMLDivElement | undefined;
    onFloatDragStart?: (e: PointerEvent) => void;
    onFloatDock?: () => void;
    onFloatClose?: () => void;
    redraw: () => void;
    setDragNode: (event: DragEvent, node: Node) => void;
    clearDragMain: () => void;
    getRevision: () => number;
    showPopup: (
        triggerElement: HTMLElement,
        parentNode: TabSetNode | BorderNode,
        items: { index: number; node: TabNode }[],
        onSelect: (item: { index: number; node: TabNode }) => void,
    ) => void;
    showContextMenu: (
        event: MouseEvent,
        items: { label: string; action: () => void }[],
    ) => void;
}
