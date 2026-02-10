import { Action, type LayoutAction } from "../model/Action";
import { CLASSES } from "../core/Types";
import { Model } from "../model/Model";
import { TabNode } from "../model/TabNode";
import type { IContentRenderer } from "./IContentRenderer";

export interface IFloatingWindowManagerOptions {
    model: Model;
    root: HTMLElement;
    getClassName(defaultClassName: string): string;
    doAction(action: LayoutAction): void;
    createContentRenderer(node: TabNode): IContentRenderer;
}

export class VanillaFloatingWindowManager {
    private panels = new Map<string, HTMLElement>();

    constructor(private readonly options: IFloatingWindowManagerOptions) {}

    render(): void {
        const windows = [...this.options.model.getwindowsMap().values()].filter((w) => w.windowType === "float");
        const seen = new Set<string>();

        for (const win of windows) {
            seen.add(win.windowId);
            let panel = this.panels.get(win.windowId);
            if (!panel) {
                panel = document.createElement("div");
                panel.className = this.options.getClassName(CLASSES.FLEXLAYOUT__FLOATING_PANEL);
                panel.dataset.windowId = win.windowId;
                this.options.root.appendChild(panel);
                this.panels.set(win.windowId, panel);
            }

            panel.style.left = `${win.rect.x}px`;
            panel.style.top = `${win.rect.y}px`;
            panel.style.width = `${win.rect.width}px`;
            panel.style.height = `${win.rect.height}px`;
            panel.style.zIndex = "1000";
        }

        for (const [windowId, panel] of this.panels) {
            if (!seen.has(windowId)) {
                panel.remove();
                this.panels.delete(windowId);
            }
        }
    }

    closeWindow(windowId: string): void {
        this.options.doAction(Action.closeWindow(windowId));
    }

    dispose(): void {
        for (const [, panel] of this.panels) {
            panel.remove();
        }
        this.panels.clear();
    }
}
