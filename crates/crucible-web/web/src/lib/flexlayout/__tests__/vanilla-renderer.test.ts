import { describe, expect, it } from "vitest";
import { Model } from "../model/Model";
import { TabNode } from "../model/TabNode";
import type { IContentRenderer, IRenderParams } from "../rendering/IContentRenderer";
import { VanillaLayoutRenderer } from "../rendering/VanillaLayoutRenderer";

class TestContentRenderer implements IContentRenderer {
    private container: HTMLElement | undefined;
    private params: IRenderParams | undefined;

    init(container: HTMLElement, params: IRenderParams): void {
        this.container = container;
        this.params = params;
        this.render();
    }

    update(params: Partial<IRenderParams>): void {
        if (!this.params) {
            return;
        }
        this.params = { ...this.params, ...params };
        this.render();
    }

    dispose(): void {
        if (this.container) {
            this.container.textContent = "";
        }
    }

    private render(): void {
        if (!this.container || !this.params) {
            return;
        }
        this.container.textContent = this.params.node.getName();
    }
}

describe("VanillaLayoutRenderer", () => {
    it("mounts and unmounts in jsdom", () => {
        const model = Model.fromJson({
            global: {},
            layout: {
                type: "row",
                children: [
                    {
                        type: "tabset",
                        children: [
                            { type: "tab", id: "tab-1", name: "Tab One", component: "test" },
                        ],
                    },
                ],
            },
        });

        const host = document.createElement("div");
        host.style.width = "1000px";
        host.style.height = "600px";
        document.body.appendChild(host);

        const renderer = new VanillaLayoutRenderer({
            model,
            getClassName: (className) => className,
            doAction: (action) => model.doAction(action),
            createContentRenderer: (_node: TabNode) => new TestContentRenderer(),
        });

        renderer.mount(host);

        const root = host.querySelector('[data-layout-path="/"]');
        expect(root).toBeTruthy();

        renderer.unmount();

        const afterUnmount = host.querySelector('[data-layout-path="/"]');
        expect(afterUnmount).toBeNull();
    });
});
