import { CompositeDisposable } from '../lifecycle';
import { DockviewGroupPanel, IDockviewGroupPanel } from './dockviewGroupPanel';
import { DockedSide } from './dockviewGroupPanelModel';

export interface IDockviewDockedGroupPanel {
    readonly group: IDockviewGroupPanel;
    readonly side: DockedSide;
    readonly collapsed: boolean;
    readonly size: number;
    setCollapsed(collapsed: boolean): void;
    setSize(size: number): void;
}

export class DockviewDockedGroupPanel
    extends CompositeDisposable
    implements IDockviewDockedGroupPanel
{
    readonly element: HTMLElement;
    private _collapsed: boolean;
    private _size: number;

    constructor(
        readonly group: DockviewGroupPanel,
        readonly side: DockedSide,
        options: { size?: number; collapsed?: boolean }
    ) {
        super();
        this._size = options.size ?? 300;
        this._collapsed = options.collapsed ?? false;
        
        this.element = document.createElement('div');
        this.element.className = `dv-docked-panel dv-docked-${side}`;
        
        this.addDisposables(group);
    }

    get collapsed(): boolean {
        return this._collapsed;
    }

    get size(): number {
        return this._size;
    }

    setCollapsed(collapsed: boolean): void {
        this._collapsed = collapsed;
    }

    setSize(size: number): void {
        this._size = size;
    }
}
