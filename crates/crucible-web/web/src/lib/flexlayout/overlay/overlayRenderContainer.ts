export interface IPanel {
    id: string;
    element: HTMLElement;
    referenceElement: HTMLElement;
    isVisible?: boolean;
    overlayElement?: HTMLElement;
}

class PositionCache {
    private cache = new Map<Element, { rect: { left: number; top: number; width: number; height: number }; frameId: number }>();
    private currentFrameId = 0;
    private rafId: number | null = null;

    getPosition(element: Element): { left: number; top: number; width: number; height: number } {
        const cached = this.cache.get(element);
        if (cached && cached.frameId === this.currentFrameId) {
            return cached.rect;
        }

        this.scheduleFrameUpdate();
        const rect = element.getBoundingClientRect();
        const position = {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        };
        this.cache.set(element, { rect: position, frameId: this.currentFrameId });
        return position;
    }

    invalidate(): void {
        this.currentFrameId++;
    }

    private scheduleFrameUpdate(): void {
        if (this.rafId) return;
        this.rafId = requestAnimationFrame(() => {
            this.currentFrameId++;
            this.rafId = null;
        });
    }
}

export class OverlayRenderContainer {
    private readonly map: Record<
        string,
        {
            panel: IPanel;
            element: HTMLElement;
            resize?: () => void;
        }
    > = {};

    private disposed = false;
    private readonly positionCache = new PositionCache();
    private readonly pendingUpdates = new Set<string>();

    constructor(readonly element: HTMLElement) {}

    updateAllPositions(): void {
        if (this.disposed) {
            return;
        }

        this.positionCache.invalidate();

        for (const entry of Object.values(this.map)) {
            if (entry.panel.isVisible && entry.resize) {
                entry.resize();
            }
        }
    }

    detach(panel: IPanel): boolean {
        if (this.map[panel.id]) {
            const { element } = this.map[panel.id];
            if (panel.element.parentElement === element) {
                element.removeChild(panel.element);
            }
            if (element.parentElement === this.element) {
                this.element.removeChild(element);
            }
            delete this.map[panel.id];
            return true;
        }
        return false;
    }

    attach(panel: IPanel): HTMLElement {
        if (!this.map[panel.id]) {
            const containerElement = document.createElement('div');
            containerElement.className = 'dv-render-overlay';
            containerElement.tabIndex = -1;

            this.map[panel.id] = {
                panel,
                element: containerElement,
            };
        }

        const containerElement = this.map[panel.id].element;

        if (panel.element.parentElement !== containerElement) {
            containerElement.appendChild(panel.element);
        }

        if (containerElement.parentElement !== this.element) {
            this.element.appendChild(containerElement);
        }

        const resize = () => {
            const panelId = panel.id;

            if (this.pendingUpdates.has(panelId)) {
                return;
            }

            this.pendingUpdates.add(panelId);

            requestAnimationFrame(() => {
                this.pendingUpdates.delete(panelId);

                if (this.disposed || !this.map[panelId]) {
                    return;
                }

                const box = this.positionCache.getPosition(panel.referenceElement);
                const box2 = this.positionCache.getPosition(this.element);

                const left = box.left - box2.left;
                const top = box.top - box2.top;
                const width = box.width;
                const height = box.height;

                containerElement.style.left = `${left}px`;
                containerElement.style.top = `${top}px`;
                containerElement.style.width = `${width}px`;
                containerElement.style.height = `${height}px`;
            });
        };

        const visibilityChanged = () => {
            if (panel.isVisible) {
                this.positionCache.invalidate();
                resize();
            }

            containerElement.style.display = panel.isVisible ? '' : 'none';
        };

        const correctLayerPosition = () => {
            if (panel.overlayElement) {
                queueMicrotask(() => {
                    const element = panel.overlayElement!;
                    const update = () => {
                        const level = Number(element.getAttribute('aria-level'));
                        containerElement.style.zIndex = `calc(var(--dv-overlay-z-index, 999) + ${level * 2 + 1})`;
                    };

                    const observer = new MutationObserver(() => {
                        update();
                    });

                    observer.observe(element, {
                        attributeFilter: ['aria-level'],
                        attributes: true,
                    });

                    update();
                });
            } else {
                containerElement.style.zIndex = '';
            }
        };

        correctLayerPosition();

        queueMicrotask(() => {
            if (this.disposed) {
                return;
            }

            visibilityChanged();
        });

        this.map[panel.id].resize = resize;

        return containerElement;
    }

    dispose(): void {
        for (const value of Object.values(this.map)) {
            if (value.panel.element.parentElement === value.element) {
                value.element.removeChild(value.panel.element);
            }
            if (value.element.parentElement === this.element) {
                this.element.removeChild(value.element);
            }
        }
        this.disposed = true;
    }

    get isDisposed(): boolean {
        return this.disposed;
    }
}
