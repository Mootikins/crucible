import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest';
import { OverlayRenderContainer } from '../overlay/overlayRenderContainer';

function mockGetBoundingClientRect(rect: Partial<DOMRect>): DOMRect {
    return {
        left: rect.left ?? 0,
        top: rect.top ?? 0,
        right: (rect.left ?? 0) + (rect.width ?? 0),
        bottom: (rect.top ?? 0) + (rect.height ?? 0),
        width: rect.width ?? 0,
        height: rect.height ?? 0,
        x: rect.left ?? 0,
        y: rect.top ?? 0,
        toJSON: () => ({}),
    };
}

function exhaustMicrotaskQueue(): Promise<void> {
    return new Promise((resolve) => {
        queueMicrotask(() => resolve());
    });
}

function exhaustAnimationFrame(): Promise<void> {
    return new Promise((resolve) => {
        requestAnimationFrame(() => resolve());
    });
}

describe('overlayRenderContainer', () => {
    let parentContainer: HTMLElement;

    beforeEach(() => {
        parentContainer = document.createElement('div');
        document.body.appendChild(parentContainer);
    });

    afterEach(() => {
        document.body.removeChild(parentContainer);
    });

    test('that attach(...) and detach(...) mutate the DOM as expected', () => {
        const container = new OverlayRenderContainer(parentContainer);

        const panelContentEl = document.createElement('div');
        const referenceElement = document.createElement('div');

        const panel = {
            id: 'test_panel_id',
            element: panelContentEl,
            referenceElement,
        };

        const containerEl = container.attach(panel);

        expect(panelContentEl.parentElement?.parentElement).toBe(parentContainer);

        container.detach(panel);

        expect(panelContentEl.parentElement?.parentElement).toBeUndefined();
    });

    test('add a view that is not currently in the DOM', async () => {
        const container = new OverlayRenderContainer(parentContainer);

        const panelContentEl = document.createElement('div');
        const referenceElement = document.createElement('div');

        const panel = {
            id: 'test_panel_id',
            element: panelContentEl,
            referenceElement,
            isVisible: true,
        };

        vi.spyOn(parentContainer, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 100,
                top: 200,
                width: 1000,
                height: 500,
            })
        );

        vi.spyOn(referenceElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 150,
                top: 300,
                width: 100,
                height: 200,
            })
        );

        const containerEl = container.attach(panel);

        await exhaustMicrotaskQueue();
        await exhaustAnimationFrame();

        expect(panelContentEl.parentElement).toBe(containerEl);
        expect(containerEl.parentElement).toBe(parentContainer);
        expect(containerEl.style.display).toBe('');
        expect(containerEl.style.left).toBe('50px');
        expect(containerEl.style.top).toBe('100px');
        expect(containerEl.style.width).toBe('100px');
        expect(containerEl.style.height).toBe('200px');
    });

    test('related z-index from `aria-level` set on floating panels', async () => {
        const container = new OverlayRenderContainer(parentContainer);

        const panelContentEl = document.createElement('div');
        const referenceElement = document.createElement('div');
        const overlayElement = document.createElement('div');
        overlayElement.setAttribute('aria-level', '2');

        const panel = {
            id: 'test_panel_id',
            element: panelContentEl,
            referenceElement,
            isVisible: true,
            overlayElement,
        };

        vi.spyOn(parentContainer, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 100,
                top: 200,
                width: 1000,
                height: 500,
            })
        );

        vi.spyOn(referenceElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 150,
                top: 300,
                width: 100,
                height: 200,
            })
        );

        const containerEl = container.attach(panel);

        await exhaustMicrotaskQueue();

        expect(containerEl.style.zIndex).toBe('calc(var(--dv-overlay-z-index, 999) + 5)');
    });

    test('that frequent resize calls are batched to prevent shaking', async () => {
        const container = new OverlayRenderContainer(parentContainer);

        const panelContentEl = document.createElement('div');
        const referenceElement = document.createElement('div');

        const panel = {
            id: 'test_panel_id',
            element: panelContentEl,
            referenceElement,
            isVisible: true,
        };

        vi.spyOn(referenceElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 100,
                top: 200,
                width: 150,
                height: 250,
            })
        );

        vi.spyOn(parentContainer, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 50,
                top: 100,
                width: 200,
                height: 300,
            })
        );

        const containerEl = container.attach(panel);

        await exhaustMicrotaskQueue();
        await exhaustAnimationFrame();

        expect(containerEl.style.left).toBe('50px');
        expect(containerEl.style.top).toBe('100px');

        vi.clearAllMocks();

        panel.referenceElement.dispatchEvent(new Event('resize'));
        panel.referenceElement.dispatchEvent(new Event('resize'));
        panel.referenceElement.dispatchEvent(new Event('resize'));
        panel.referenceElement.dispatchEvent(new Event('resize'));
        panel.referenceElement.dispatchEvent(new Event('resize'));

        await exhaustAnimationFrame();

        expect(containerEl.style.left).toBe('50px');
        expect(containerEl.style.top).toBe('100px');
        expect(containerEl.style.width).toBe('150px');
        expect(containerEl.style.height).toBe('250px');
    });

    test('updateAllPositions forces position recalculation for visible panels', async () => {
        const container = new OverlayRenderContainer(parentContainer);

        const panelContentEl1 = document.createElement('div');
        const referenceElement1 = document.createElement('div');

        const panelContentEl2 = document.createElement('div');
        const referenceElement2 = document.createElement('div');

        const panel1 = {
            id: 'panel1',
            element: panelContentEl1,
            referenceElement: referenceElement1,
            isVisible: true,
        };

        const panel2 = {
            id: 'panel2',
            element: panelContentEl2,
            referenceElement: referenceElement2,
            isVisible: false,
        };

        vi.spyOn(referenceElement1, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 100,
                top: 200,
                width: 150,
                height: 250,
            })
        );

        vi.spyOn(referenceElement2, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 100,
                top: 200,
                width: 150,
                height: 250,
            })
        );

        vi.spyOn(parentContainer, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 50,
                top: 100,
                width: 200,
                height: 300,
            })
        );

        const containerEl1 = container.attach(panel1);
        const containerEl2 = container.attach(panel2);

        await exhaustMicrotaskQueue();
        await exhaustAnimationFrame();

        vi.clearAllMocks();

        container.updateAllPositions();

        await exhaustAnimationFrame();

        expect(containerEl1.style.left).toBe('50px');
        expect(containerEl1.style.top).toBe('100px');
        expect(containerEl1.style.width).toBe('150px');
        expect(containerEl1.style.height).toBe('250px');

        expect(referenceElement1.getBoundingClientRect).toHaveBeenCalled();
        expect(parentContainer.getBoundingClientRect).toHaveBeenCalled();
    });
});
