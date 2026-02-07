import { DockviewComponent } from '../../dockview/dockviewComponent';
import {
    GroupPanelPartInitParameters,
    IContentRenderer,
} from '../../dockview/types';
import { PanelUpdateEvent } from '../../panel/types';
import { Emitter } from '../../events';

class PanelContentPartTest implements IContentRenderer {
    element: HTMLElement = document.createElement('div');

    readonly _onDidDispose = new Emitter<void>();
    readonly onDidDispose = this._onDidDispose.event;

    isDisposed: boolean = false;

    constructor(
        public readonly id: string,
        public readonly component: string
    ) {
        this.element.classList.add(`testpanel-${id}`);
    }

    init(parameters: GroupPanelPartInitParameters): void {
        //noop
    }

    layout(width: number, height: number): void {
        //noop
    }

    update(event: PanelUpdateEvent): void {
        //noop
    }

    toJSON(): object {
        return { id: this.component };
    }

    focus(): void {
        //noop
    }

    dispose(): void {
        this.isDisposed = true;
        this._onDidDispose.fire();
        this._onDidDispose.dispose();
    }
}

describe('dockviewDockedGroupPanel', () => {
    let container: HTMLElement;

    beforeEach(() => {
        container = document.createElement('div');

        window.matchMedia = window.matchMedia || function () {
            return {
                matches: false,
                addListener: function () {},
                removeListener: function () {},
                addEventListener: function () {},
                removeEventListener: function () {},
                dispatchEvent: function () { return false; },
                media: '',
                onchange: null,
            };
        };
    });

    function createDockview() {
        const dockview = new DockviewComponent(container, {
            createComponent(options) {
                switch (options.name) {
                    case 'default':
                        return new PanelContentPartTest(
                            options.id,
                            options.name
                        );
                    default:
                        throw new Error(`unsupported`);
                }
            },
        });
        dockview.layout(1000, 500);
        return dockview;
    }

    test('that a docked group can be created', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockview.dockedGroups.length).toBe(1);
        expect(dockview.dockedGroups[0].side).toBe('left');
        expect(dockview.dockedGroups[0].size).toBe(300);
        expect(dockview.dockedGroups[0].collapsed).toBe(false);
        expect(panel.group.api.location.type).toBe('docked');

        dockview.dispose();
    });

    test('that a docked group can be created from a group', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const group = panel.group;

        const dockedGroup = dockview.addDockedGroup(group, {
            side: 'right',
            size: 200,
        });

        expect(dockview.dockedGroups.length).toBe(1);
        expect(dockedGroup.side).toBe('right');
        expect(dockedGroup.size).toBe(200);
        expect(dockedGroup.collapsed).toBe(false);
        expect(group.api.location.type).toBe('docked');

        dockview.dispose();
    });

    test('that docked groups can be added to all sides', () => {
        const dockview = createDockview();

        const panelLeft = dockview.addPanel({
            id: 'panel-left',
            component: 'default',
        });
        const panelRight = dockview.addPanel({
            id: 'panel-right',
            component: 'default',
        });
        const panelTop = dockview.addPanel({
            id: 'panel-top',
            component: 'default',
        });
        const panelBottom = dockview.addPanel({
            id: 'panel-bottom',
            component: 'default',
        });

        dockview.addDockedGroup(panelLeft, { side: 'left', size: 200 });
        dockview.addDockedGroup(panelRight, { side: 'right', size: 200 });
        dockview.addDockedGroup(panelTop, { side: 'top', size: 100 });
        dockview.addDockedGroup(panelBottom, { side: 'bottom', size: 100 });

        expect(dockview.dockedGroups.length).toBe(4);
        expect(dockview.getDockedGroups('left').length).toBe(1);
        expect(dockview.getDockedGroups('right').length).toBe(1);
        expect(dockview.getDockedGroups('top').length).toBe(1);
        expect(dockview.getDockedGroups('bottom').length).toBe(1);

        dockview.dispose();
    });

    test('that a docked group can be removed', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockview.dockedGroups.length).toBe(1);

        dockview.removeDockedGroup(dockedGroup);

        expect(dockview.dockedGroups.length).toBe(0);

        dockview.dispose();
    });

    test('that a docked group can be collapsed and expanded', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockedGroup.collapsed).toBe(false);
        expect(dockedGroup.element.style.flexBasis).toBe('300px');

        dockedGroup.setCollapsed(true);

        expect(dockedGroup.collapsed).toBe(true);
        expect(dockedGroup.element.style.flexBasis).toBe('0px');

        dockedGroup.setCollapsed(false);

        expect(dockedGroup.collapsed).toBe(false);
        expect(dockedGroup.element.style.flexBasis).toBe('300px');

        dockview.dispose();
    });

    test('that setSize updates the docked group size', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockedGroup.size).toBe(300);
        expect(dockedGroup.element.style.flexBasis).toBe('300px');

        dockedGroup.setSize(400);

        expect(dockedGroup.size).toBe(400);
        expect(dockedGroup.element.style.flexBasis).toBe('400px');

        dockedGroup.setCollapsed(true);
        expect(dockedGroup.element.style.flexBasis).toBe('0px');

        dockedGroup.setSize(500);

        expect(dockedGroup.size).toBe(500);
        expect(dockedGroup.element.style.flexBasis).toBe('0px');

        dockview.dispose();
    });

    test('that toggleDockedSide collapses and expands', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockedGroup.collapsed).toBe(false);

        dockview.toggleDockedSide('left');

        expect(dockedGroup.collapsed).toBe(true);

        dockview.toggleDockedSide('left');

        expect(dockedGroup.collapsed).toBe(false);

        dockview.dispose();
    });

    test('that docked groups are serialized and deserialized', () => {
        const dockview = createDockview();

        dockview.addPanel({
            id: 'grid-panel',
            component: 'default',
        });

        const dockedPanel = dockview.addPanel({
            id: 'docked-panel',
            component: 'default',
        });

        dockview.addDockedGroup(dockedPanel, {
            side: 'left',
            size: 250,
        });

        const serialized = dockview.toJSON();

        expect(serialized.dockedGroups).toBeDefined();
        expect(serialized.dockedGroups!.length).toBe(1);
        expect(serialized.dockedGroups![0].side).toBe('left');
        expect(serialized.dockedGroups![0].size).toBe(250);
        expect(serialized.dockedGroups![0].collapsed).toBe(false);

        const newDockview = createDockview();
        newDockview.fromJSON(serialized);

        expect(newDockview.dockedGroups.length).toBe(1);
        expect(newDockview.dockedGroups[0].side).toBe('left');
        expect(newDockview.dockedGroups[0].size).toBe(250);
        expect(newDockview.dockedGroups[0].collapsed).toBe(false);

        dockview.dispose();
        newDockview.dispose();
    });

    test('that docked group element has correct CSS classes', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        const dockedGroup = dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(dockedGroup.element.className).toContain('dv-docked-panel');
        expect(dockedGroup.element.className).toContain('dv-docked-left');

        const panel2 = dockview.addPanel({
            id: 'panel2',
            component: 'default',
        });

        const dockedGroupRight = dockview.addDockedGroup(panel2, {
            side: 'right',
            size: 200,
        });

        expect(dockedGroupRight.element.className).toContain('dv-docked-panel');
        expect(dockedGroupRight.element.className).toContain('dv-docked-right');

        dockview.dispose();
    });

    test('that clear() works when docked groups exist', () => {
        const dockview = createDockview();

        dockview.addPanel({
            id: 'grid-panel',
            component: 'default',
        });

        const dockedPanel = dockview.addPanel({
            id: 'docked-panel',
            component: 'default',
        });

        dockview.addDockedGroup(dockedPanel, {
            side: 'left',
            size: 250,
        });

        expect(dockview.dockedGroups.length).toBe(1);
        expect(dockview.groups.length).toBe(2);

        dockview.clear();

        expect(dockview.dockedGroups.length).toBe(0);
        expect(dockview.groups.length).toBe(0);

        dockview.dispose();
    });

    test('that fromJSON works on a dockview that already has docked groups', () => {
        const dockview = createDockview();

        dockview.addPanel({
            id: 'grid-panel',
            component: 'default',
        });

        const dockedPanel = dockview.addPanel({
            id: 'docked-panel',
            component: 'default',
        });

        dockview.addDockedGroup(dockedPanel, {
            side: 'left',
            size: 250,
        });

        const serialized = dockview.toJSON();

        dockview.fromJSON(serialized);

        expect(dockview.dockedGroups.length).toBe(1);
        expect(dockview.dockedGroups[0].side).toBe('left');
        expect(dockview.dockedGroups[0].size).toBe(250);

        dockview.dispose();
    });

    test('that panel location reports docked type', () => {
        const dockview = createDockview();

        const panel = dockview.addPanel({
            id: 'panel1',
            component: 'default',
        });

        expect(panel.api.location.type).toBe('grid');

        dockview.addDockedGroup(panel, {
            side: 'left',
            size: 300,
        });

        expect(panel.api.location.type).toBe('docked');

        dockview.dispose();
    });
});
