/**
 * Content renderer lifecycle contract (Dockview pattern).
 *
 * Framework-agnostic lifecycle for rendering content into a container
 * DOM element. Implementations mount their framework (SolidJS, React, etc.)
 * inside `init`, update on parameter changes, and clean up in `dispose`.
 *
 * This decouples the layout engine from any specific rendering framework.
 */

import type { TabNode } from "../model/TabNode";

export interface IRenderParams {
    node: TabNode;
    selected: boolean;
    windowId: string;
}

export interface IContentRenderer {
    init(container: HTMLElement, params: IRenderParams): void;
    update(params: Partial<IRenderParams>): void;
    dispose(): void;
}
