import { describe, it, expect, beforeEach } from 'vitest';
import { DockedPanel, DockedSide } from '../docked/DockedPanel';

describe('dockviewDockedGroupPanel', () => {
    let container: HTMLElement;

    beforeEach(() => {
        container = document.createElement('div');
        document.body.appendChild(container);

        // Mock matchMedia for prefers-reduced-motion
        if (!window.matchMedia) {
            (window as any).matchMedia = function () {
                return {
                    matches: false,
                    addListener: function () {},
                    removeListener: function () {},
                    addEventListener: function () {},
                    removeEventListener: function () {},
                    dispatchEvent: function () {
                        return false;
                    },
                    media: '',
                    onchange: null,
                };
            };
        }
    });

    it('that a docked panel can be created', () => {
        const panel = new DockedPanel('left', { size: 300 });

        expect(panel.side).toBe('left');
        expect(panel.size).toBe(300);
        expect(panel.collapsed).toBe(false);
        expect(panel.element.className).toContain('dv-docked-panel');
        expect(panel.element.className).toContain('dv-docked-left');
    });

    it('that docked panels can be created for all sides', () => {
        const panelLeft = new DockedPanel('left', { size: 200 });
        const panelRight = new DockedPanel('right', { size: 200 });
        const panelTop = new DockedPanel('top', { size: 100 });
        const panelBottom = new DockedPanel('bottom', { size: 100 });

        expect(panelLeft.side).toBe('left');
        expect(panelRight.side).toBe('right');
        expect(panelTop.side).toBe('top');
        expect(panelBottom.side).toBe('bottom');

        expect(panelLeft.element.className).toContain('dv-docked-left');
        expect(panelRight.element.className).toContain('dv-docked-right');
        expect(panelTop.element.className).toContain('dv-docked-top');
        expect(panelBottom.element.className).toContain('dv-docked-bottom');
    });

    it('that a docked panel can be removed', () => {
        const panel = new DockedPanel('left', { size: 300 });
        container.appendChild(panel.element);

        expect(container.contains(panel.element)).toBe(true);

        panel.dispose();

        expect(container.contains(panel.element)).toBe(false);
    });

    it('that a docked panel can be collapsed and expanded', () => {
        const panel = new DockedPanel('left', { size: 300 });

        expect(panel.collapsed).toBe(false);
        expect(panel.element.style.flexBasis).toBe('300px');

        panel.setCollapsed(true);

        expect(panel.collapsed).toBe(true);
        expect(panel.element.style.flexBasis).toBe('0px');

        panel.setCollapsed(false);

        expect(panel.collapsed).toBe(false);
        expect(panel.element.style.flexBasis).toBe('300px');
    });

    it('that setSize updates the docked panel size', () => {
        const panel = new DockedPanel('left', { size: 300 });

        expect(panel.size).toBe(300);
        expect(panel.element.style.flexBasis).toBe('300px');

        panel.setSize(400);

        expect(panel.size).toBe(400);
        expect(panel.element.style.flexBasis).toBe('400px');

        panel.setCollapsed(true);
        expect(panel.element.style.flexBasis).toBe('0px');

        panel.setSize(500);

        expect(panel.size).toBe(500);
        expect(panel.element.style.flexBasis).toBe('0px');
    });

    it('that toggleCollapsed collapses and expands', () => {
        const panel = new DockedPanel('left', { size: 300 });

        expect(panel.collapsed).toBe(false);

        panel.toggleCollapsed();

        expect(panel.collapsed).toBe(true);

        panel.toggleCollapsed();

        expect(panel.collapsed).toBe(false);
    });

    it('that docked panels are serialized', () => {
        const panel = new DockedPanel('left', { size: 250, collapsed: false });

        const serialized = panel.toJSON();

        expect(serialized.side).toBe('left');
        expect(serialized.size).toBe(250);
        expect(serialized.collapsed).toBe(false);
    });

    it('that docked panels are deserialized', () => {
        const serialized = { side: 'right' as DockedSide, size: 200, collapsed: true };

        const panel = DockedPanel.fromJSON(serialized);

        expect(panel.side).toBe('right');
        expect(panel.size).toBe(200);
        expect(panel.collapsed).toBe(true);
        expect(panel.element.style.flexBasis).toBe('0px');
    });

    it('that docked panel element has correct CSS classes', () => {
        const panelLeft = new DockedPanel('left', { size: 300 });
        const panelRight = new DockedPanel('right', { size: 300 });

        expect(panelLeft.element.className).toContain('dv-docked-panel');
        expect(panelLeft.element.className).toContain('dv-docked-left');

        expect(panelRight.element.className).toContain('dv-docked-panel');
        expect(panelRight.element.className).toContain('dv-docked-right');
    });

    it('that docked panel respects prefers-reduced-motion', () => {
        // Mock matchMedia to return true for prefers-reduced-motion
        const originalMatchMedia = window.matchMedia;
        (window as any).matchMedia = function (query: string) {
            return {
                matches: query === '(prefers-reduced-motion: reduce)',
                addListener: function () {},
                removeListener: function () {},
                addEventListener: function () {},
                removeEventListener: function () {},
                dispatchEvent: function () {
                    return false;
                },
                media: query,
                onchange: null,
            };
        };

        const panel = new DockedPanel('left', { size: 300 });

        // Should have minimal transition duration
        const transitionDuration = panel.element.style.transition;
        expect(transitionDuration).toContain('0.01ms');

        // Restore original matchMedia
        (window as any).matchMedia = originalMatchMedia;
    });

    it('that docked panel has correct flex properties', () => {
        const panel = new DockedPanel('left', { size: 300 });

        expect(panel.element.style.display).toBe('flex');
        expect(panel.element.style.flexDirection).toBe('column');
        expect(panel.element.style.flexShrink).toBe('0');
        expect(panel.element.style.flexGrow).toBe('0');
        expect(panel.element.style.overflow).toBe('hidden');
    });

    it('that docked panel initializes with collapsed state', () => {
        const panelExpanded = new DockedPanel('left', { size: 300, collapsed: false });
        const panelCollapsed = new DockedPanel('right', { size: 200, collapsed: true });

        expect(panelExpanded.collapsed).toBe(false);
        expect(panelExpanded.element.style.flexBasis).toBe('300px');

        expect(panelCollapsed.collapsed).toBe(true);
        expect(panelCollapsed.element.style.flexBasis).toBe('0px');
    });
});
