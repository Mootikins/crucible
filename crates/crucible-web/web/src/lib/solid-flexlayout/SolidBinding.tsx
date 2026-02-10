import { Component, onCleanup, onMount } from "solid-js";
import { render } from "solid-js/web";
import { Layout as LegacyLayout } from "../solid-flexlayout-legacy/Layout";
import type { ILayoutProps } from "./LayoutTypes";
import { VanillaLayoutRenderer } from "../flexlayout/rendering/VanillaLayoutRenderer";
import type { IContentRenderer, IRenderParams } from "../flexlayout/rendering/IContentRenderer";

class SolidContentRenderer implements IContentRenderer {
    private container: HTMLElement | undefined;
    private params: IRenderParams | undefined;
    private disposeFn: (() => void) | undefined;

    constructor(private readonly factory: ILayoutProps["factory"]) {}

    init(container: HTMLElement, params: IRenderParams): void {
        this.container = container;
        this.params = params;
        this.mount();
    }

    update(params: Partial<IRenderParams>): void {
        if (!this.params) {
            return;
        }
        this.params = { ...this.params, ...params };
        this.mount();
    }

    dispose(): void {
        if (this.disposeFn) {
            this.disposeFn();
            this.disposeFn = undefined;
        }
        this.container = undefined;
        this.params = undefined;
    }

    private mount(): void {
        if (!this.container || !this.params) {
            return;
        }
        if (this.disposeFn) {
            this.disposeFn();
            this.disposeFn = undefined;
        }
        this.disposeFn = render(() => this.factory(this.params!.node), this.container);
    }
}

export const SolidBinding: Component<ILayoutProps> = (props) => {
    let containerRef: HTMLDivElement | undefined;
    let renderer: VanillaLayoutRenderer | undefined;

    const useVanilla = () => {
        const config = window as Window & { __FLEXLAYOUT_VANILLA__?: string };
        return config.__FLEXLAYOUT_VANILLA__ === "1";
    };

    onMount(() => {
        if (!useVanilla() || !containerRef) {
            return;
        }

        renderer = new VanillaLayoutRenderer({
            model: props.model,
            getClassName: (className) => props.classNameMapper?.(className) ?? className,
            doAction: (action) => {
                props.model.doAction(action);
            },
            onModelChange: props.onModelChange,
            onAction: props.onAction,
            createContentRenderer: () => new SolidContentRenderer(props.factory),
        });

        renderer.mount(containerRef);
    });

    onCleanup(() => {
        renderer?.unmount();
        renderer = undefined;
    });

    if (!useVanilla()) {
        return <LegacyLayout {...props} />;
    }

    return <div ref={containerRef} />;
};
