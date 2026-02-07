export type DockedSide = 'left' | 'right' | 'top' | 'bottom';

export interface IDockedPanelOptions {
    size?: number;
    collapsed?: boolean;
}

export interface IDockedPanelJSON {
    side: DockedSide;
    size: number;
    collapsed: boolean;
}

export class DockedPanel {
    readonly element: HTMLElement;
    private _collapsed: boolean;
    private _size: number;

    constructor(
        readonly side: DockedSide,
        options: IDockedPanelOptions = {}
    ) {
        this._size = options.size ?? 300;
        this._collapsed = options.collapsed ?? false;

        this.element = document.createElement('div');
        this.element.className = `dv-docked-panel dv-docked-${side}`;

        const prefersReducedMotion = window.matchMedia(
            '(prefers-reduced-motion: reduce)'
        ).matches;
        const transitionDuration = prefersReducedMotion ? '0.01ms' : '200ms';
        this.element.style.transition = `flex-basis ${transitionDuration} ease-out`;

        this.element.style.display = 'flex';
        this.element.style.flexDirection = 'column';
        this.element.style.flexShrink = '0';
        this.element.style.flexGrow = '0';
        this.element.style.overflow = 'hidden';

        if (this._collapsed) {
            this.element.style.flexBasis = '0';
        } else {
            this.element.style.flexBasis = `${this._size}px`;
        }
    }

    get collapsed(): boolean {
        return this._collapsed;
    }

    get size(): number {
        return this._size;
    }

    setCollapsed(collapsed: boolean): void {
        this._collapsed = collapsed;

        if (collapsed) {
            this.element.style.flexBasis = '0';
        } else {
            this.element.style.flexBasis = `${this._size}px`;
        }
    }

    setSize(size: number): void {
        this._size = size;

        if (!this._collapsed) {
            this.element.style.flexBasis = `${size}px`;
        }
    }

    toggleCollapsed(): void {
        this.setCollapsed(!this._collapsed);
    }

    toJSON(): IDockedPanelJSON {
        return {
            side: this.side,
            size: this._size,
            collapsed: this._collapsed,
        };
    }

    static fromJSON(json: IDockedPanelJSON): DockedPanel {
        return new DockedPanel(json.side, {
            size: json.size,
            collapsed: json.collapsed,
        });
    }

    dispose(): void {
        if (this.element.parentElement) {
            this.element.parentElement.removeChild(this.element);
        }
    }
}
