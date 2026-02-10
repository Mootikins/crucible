import { JSX } from "solid-js";
import { Model } from "../flexlayout/model/Model";
import { Node } from "../flexlayout/model/Node";
import { TabNode } from "../flexlayout/model/TabNode";
import { TabSetNode } from "../flexlayout/model/TabSetNode";
import { BorderNode } from "../flexlayout/model/BorderNode";

export interface IMenuItem {
    label: string;
    action: () => void;
}

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
    onContextMenu?: (node: TabNode, event: MouseEvent) => IMenuItem[];
    onAllowDrop?: (dragNode: Node, dropInfo: any) => boolean;
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
