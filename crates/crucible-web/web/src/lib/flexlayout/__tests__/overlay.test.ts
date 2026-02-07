/// <reference lib="dom" />
import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest';
import { Overlay } from '../overlay/overlay';

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

describe('Overlay', () => {
    let container: HTMLElement;
    let content: HTMLElement;

    beforeEach(() => {
        container = document.createElement('div');
        content = document.createElement('div');
        document.body.appendChild(container);
        container.appendChild(content);
    });

    afterEach(() => {
        document.body.removeChild(container);
    });

    test('toJSON, top left', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 80,
                top: 100,
                width: 40,
                height: 50,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 20,
                top: 30,
                width: 100,
                height: 100,
            })
        );

        overlay.setBounds();

        expect(overlay.toJSON()).toEqual({
            top: 70,
            left: 60,
            width: 40,
            height: 50,
        });

        overlay.dispose();
    });

    test('toJSON, bottom right', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            right: 10,
            bottom: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 80,
                top: 100,
                width: 40,
                height: 50,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 20,
                top: 30,
                width: 100,
                height: 100,
            })
        );

        overlay.setBounds();

        expect(overlay.toJSON()).toEqual({
            bottom: -20,
            right: 0,
            width: 40,
            height: 50,
        });

        overlay.dispose();
    });

    test('that out-of-bounds dimensions are fixed, top left', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: -1000,
            top: -1000,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 80,
                top: 100,
                width: 40,
                height: 50,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 20,
                top: 30,
                width: 100,
                height: 100,
            })
        );

        overlay.setBounds();

        expect(overlay.toJSON()).toEqual({
            top: 70,
            left: 60,
            width: 40,
            height: 50,
        });

        overlay.dispose();
    });

    test('that out-of-bounds dimensions are fixed, bottom right', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            bottom: -1000,
            right: -1000,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 80,
                top: 100,
                width: 40,
                height: 50,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 20,
                top: 30,
                width: 100,
                height: 100,
            })
        );

        overlay.setBounds();

        expect(overlay.toJSON()).toEqual({
            bottom: -20,
            right: 0,
            width: 40,
            height: 50,
        });

        overlay.dispose();
    });

    test('setBounds, top left', () => {
        const overlay = new Overlay({
            height: 1000,
            width: 1000,
            left: 0,
            top: 0,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 300,
                top: 400,
                width: 200,
                height: 100,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 0,
                top: 0,
                width: 1000,
                height: 1000,
            })
        );

        overlay.setBounds({ height: 100, width: 200, left: 300, top: 400 });

        expect(overlayElement.style.height).toBe('100px');
        expect(overlayElement.style.width).toBe('200px');
        expect(overlayElement.style.left).toBe('300px');
        expect(overlayElement.style.top).toBe('400px');

        overlay.dispose();
    });

    test('setBounds, bottom right', () => {
        const overlay = new Overlay({
            height: 1000,
            width: 1000,
            right: 0,
            bottom: 0,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 500,
                top: 500,
                width: 200,
                height: 100,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 0,
                top: 0,
                width: 1000,
                height: 1000,
            })
        );

        overlay.setBounds({ height: 100, width: 200, right: 300, bottom: 400 });

        expect(overlayElement.style.height).toBe('100px');
        expect(overlayElement.style.width).toBe('200px');
        expect(overlayElement.style.right).toBe('300px');
        expect(overlayElement.style.bottom).toBe('400px');

        overlay.dispose();
    });

    test('bringToFront updates aria-level', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        overlay.bringToFront();

        expect(overlayElement.getAttribute('aria-level')).toBeTruthy();

        overlay.dispose();
    });

    test('setVisible toggles dv-hidden class', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        overlay.setVisible(false);
        expect(overlayElement.classList.contains('dv-hidden')).toBe(true);

        overlay.setVisible(true);
        expect(overlayElement.classList.contains('dv-hidden')).toBe(false);

        overlay.dispose();
    });

    test('isVisible property reflects visibility state', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        expect(overlay.isVisible).toBe(true);

        overlay.setVisible(false);
        expect(overlay.isVisible).toBe(false);

        overlay.setVisible(true);
        expect(overlay.isVisible).toBe(true);

        overlay.dispose();
    });

    test('element property returns the overlay container element', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlay.element).toBe(overlayElement);

        overlay.dispose();
    });

    test('resize handles are created for all 8 directions', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        const directions = ['top', 'bottom', 'left', 'right', 'topleft', 'topright', 'bottomleft', 'bottomright'];
        for (const direction of directions) {
            const handle = overlayElement.querySelector(`.dv-resize-handle-${direction}`);
            expect(handle).toBeTruthy();
        }

        overlay.dispose();
    });

    test('onDidChange event fires when bounds change', () => {
        const overlay = new Overlay({
            height: 200,
            width: 100,
            left: 10,
            top: 20,
            minimumInViewportWidth: 0,
            minimumInViewportHeight: 0,
            container,
            content,
        });

        const overlayElement = container.querySelector('.dv-resize-container') as HTMLElement;
        expect(overlayElement).toBeTruthy();

        vi.spyOn(overlayElement, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 80,
                top: 100,
                width: 40,
                height: 50,
            })
        );
        vi.spyOn(container, 'getBoundingClientRect').mockReturnValue(
            mockGetBoundingClientRect({
                left: 20,
                top: 30,
                width: 100,
                height: 100,
            })
        );

        let changeCount = 0;
        overlay.onDidChange(() => {
            changeCount++;
        });

        overlay.setBounds({ height: 100, width: 200 });
        expect(changeCount).toBeGreaterThan(0);

        overlay.dispose();
    });
});
