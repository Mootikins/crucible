import type { AnchoredBox } from '../types';

class AriaLevelTracker {
    private orderedList: HTMLElement[] = [];

    push(element: HTMLElement): void {
        this.orderedList = [
            ...this.orderedList.filter((item) => item !== element),
            element,
        ];
        this.update();
    }

    destroy(element: HTMLElement): void {
        this.orderedList = this.orderedList.filter((item) => item !== element);
        this.update();
    }

    private update(): void {
        for (let i = 0; i < this.orderedList.length; i++) {
            this.orderedList[i].setAttribute('aria-level', `${i}`);
            this.orderedList[i].style.zIndex = `calc(var(--dv-overlay-z-index, 999) + ${i * 2})`;
        }
    }
}

const ariaLevelTracker = new AriaLevelTracker();

export class Overlay {
    private readonly element: HTMLElement = document.createElement('div');
    private readonly onDidChangeCallbacks: (() => void)[] = [];
    private readonly onDidChangeEndCallbacks: (() => void)[] = [];
    private isVisible: boolean;
    private verticalAlignment: 'top' | 'bottom' | undefined;
    private horizontalAlignment: 'left' | 'right' | undefined;
    private static readonly MINIMUM_HEIGHT = 20;
    private static readonly MINIMUM_WIDTH = 20;

    constructor(
        private readonly options: AnchoredBox & {
            container: HTMLElement;
            content: HTMLElement;
            minimumInViewportWidth?: number;
            minimumInViewportHeight?: number;
        }
    ) {
        this.element.className = 'dv-resize-container';
        this.isVisible = true;

        this.setupResize('top');
        this.setupResize('bottom');
        this.setupResize('left');
        this.setupResize('right');
        this.setupResize('topleft');
        this.setupResize('topright');
        this.setupResize('bottomleft');
        this.setupResize('bottomright');

        this.element.appendChild(this.options.content);
        this.options.container.appendChild(this.element);

        this.setBounds({
            height: this.options.height,
            width: this.options.width,
            ...('top' in this.options && { top: this.options.top }),
            ...('bottom' in this.options && { bottom: this.options.bottom }),
            ...('left' in this.options && { left: this.options.left }),
            ...('right' in this.options && { right: this.options.right }),
        });

        ariaLevelTracker.push(this.element);
    }

    get isVisibleProperty(): boolean {
        return this.isVisible;
    }

    setVisible(isVisible: boolean): void {
        if (isVisible === this.isVisible) {
            return;
        }

        this.isVisible = isVisible;
        this.toggleClass(this.element, 'dv-hidden', !this.isVisible);
    }

    bringToFront(): void {
        ariaLevelTracker.push(this.element);
    }

    setBounds(bounds: Partial<AnchoredBox> = {}): void {
        if (typeof bounds.height === 'number') {
            this.element.style.height = `${bounds.height}px`;
        }
        if (typeof bounds.width === 'number') {
            this.element.style.width = `${bounds.width}px`;
        }
        if ('top' in bounds && typeof bounds.top === 'number') {
            this.element.style.top = `${bounds.top}px`;
            this.element.style.bottom = 'auto';
            this.verticalAlignment = 'top';
        }
        if ('bottom' in bounds && typeof bounds.bottom === 'number') {
            this.element.style.bottom = `${bounds.bottom}px`;
            this.element.style.top = 'auto';
            this.verticalAlignment = 'bottom';
        }
        if ('left' in bounds && typeof bounds.left === 'number') {
            this.element.style.left = `${bounds.left}px`;
            this.element.style.right = 'auto';
            this.horizontalAlignment = 'left';
        }
        if ('right' in bounds && typeof bounds.right === 'number') {
            this.element.style.right = `${bounds.right}px`;
            this.element.style.left = 'auto';
            this.horizontalAlignment = 'right';
        }

        const containerRect = this.options.container.getBoundingClientRect();
        const overlayRect = this.element.getBoundingClientRect();

        const xOffset = Math.max(0, this.getMinimumWidth(overlayRect.width));
        const yOffset = Math.max(0, this.getMinimumHeight(overlayRect.height));

        if (this.verticalAlignment === 'top') {
            const top = this.clamp(
                overlayRect.top - containerRect.top,
                -yOffset,
                Math.max(0, containerRect.height - overlayRect.height + yOffset)
            );
            this.element.style.top = `${top}px`;
            this.element.style.bottom = 'auto';
        }

        if (this.verticalAlignment === 'bottom') {
            const bottom = this.clamp(
                containerRect.bottom - overlayRect.bottom,
                -yOffset,
                Math.max(0, containerRect.height - overlayRect.height + yOffset)
            );
            this.element.style.bottom = `${bottom}px`;
            this.element.style.top = 'auto';
        }

        if (this.horizontalAlignment === 'left') {
            const left = this.clamp(
                overlayRect.left - containerRect.left,
                -xOffset,
                Math.max(0, containerRect.width - overlayRect.width + xOffset)
            );
            this.element.style.left = `${left}px`;
            this.element.style.right = 'auto';
        }

        if (this.horizontalAlignment === 'right') {
            const right = this.clamp(
                containerRect.right - overlayRect.right,
                -xOffset,
                Math.max(0, containerRect.width - overlayRect.width + xOffset)
            );
            this.element.style.right = `${right}px`;
            this.element.style.left = 'auto';
        }

        this.fireOnDidChange();
    }

    toJSON(): AnchoredBox {
        const container = this.options.container.getBoundingClientRect();
        const element = this.element.getBoundingClientRect();

        const result: any = {};

        if (this.verticalAlignment === 'top') {
            result.top = parseFloat(this.element.style.top);
        } else if (this.verticalAlignment === 'bottom') {
            result.bottom = parseFloat(this.element.style.bottom);
        } else {
            result.top = element.top - container.top;
        }

        if (this.horizontalAlignment === 'left') {
            result.left = parseFloat(this.element.style.left);
        } else if (this.horizontalAlignment === 'right') {
            result.right = parseFloat(this.element.style.right);
        } else {
            result.left = element.left - container.left;
        }

        result.width = element.width;
        result.height = element.height;

        return result;
    }

    onDidChange(callback: () => void): void {
        this.onDidChangeCallbacks.push(callback);
    }

    onDidChangeEnd(callback: () => void): void {
        this.onDidChangeEndCallbacks.push(callback);
    }

    private fireOnDidChange(): void {
        for (const callback of this.onDidChangeCallbacks) {
            callback();
        }
    }

    private fireOnDidChangeEnd(): void {
        for (const callback of this.onDidChangeEndCallbacks) {
            callback();
        }
    }

    private setupResize(
        direction:
            | 'top'
            | 'bottom'
            | 'left'
            | 'right'
            | 'topleft'
            | 'topright'
            | 'bottomleft'
            | 'bottomright'
    ): void {
        const resizeHandleElement = document.createElement('div');
        resizeHandleElement.className = `dv-resize-handle-${direction}`;
        this.element.appendChild(resizeHandleElement);
    }

    private getMinimumWidth(width: number): number {
        if (typeof this.options.minimumInViewportWidth === 'number') {
            return width - this.options.minimumInViewportWidth;
        }
        return 0;
    }

    private getMinimumHeight(height: number): number {
        if (typeof this.options.minimumInViewportHeight === 'number') {
            return height - this.options.minimumInViewportHeight;
        }
        return 0;
    }

    private clamp(value: number, min: number, max: number): number {
        return Math.max(min, Math.min(max, value));
    }

    private toggleClass(element: HTMLElement, className: string, add: boolean): void {
        if (add) {
            element.classList.add(className);
        } else {
            element.classList.remove(className);
        }
    }

    dispose(): void {
        ariaLevelTracker.destroy(this.element);
        this.element.remove();
        this.onDidChangeCallbacks.length = 0;
        this.onDidChangeEndCallbacks.length = 0;
    }
}
