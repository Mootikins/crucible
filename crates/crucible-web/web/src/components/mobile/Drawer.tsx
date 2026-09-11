import { Component, JSX, createEffect, on, onCleanup } from 'solid-js';
import type { DrawerSide } from '@/components/mobile/drawer-gesture';
import { navStack } from '@/components/mobile/NavStack';

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Reduced motion turns the slide into a cut. */
const prefersReducedMotion = () =>
  typeof window.matchMedia === 'function' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * One edge drawer of the compact shell: an overlay that slides over the
 * content, never pushing it.
 *
 * It stays MOUNTED while closed, `inert` and hidden from assistive tech, so a
 * panel inside keeps its scroll position and its loaded data between visits.
 * The shell owns the swipe (`createEdgeSwipe`) and passes `dragPx` while a
 * finger holds the drawer; this component only draws that position.
 *
 * Dismissal has four doors, because a phone lacks the desktop's: Escape, a
 * scrim tap, a swipe, and the hardware back button (through `NavStack`).
 */
export const Drawer: Component<{
  side: DrawerSide;
  label: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** How far open to draw while a finger holds the drawer, or null. */
  dragPx: number | null;
  /** The drawer's width in px, which the swipe also measures against. */
  width: number;
  children: JSX.Element;
}> = (props) => {
  let panel!: HTMLDivElement;
  let opener: Element | null = null;
  let releaseBack: (() => void) | null = null;

  /** 0 = closed, 1 = open. */
  const fraction = () =>
    props.dragPx !== null ? props.dragPx / props.width : props.open ? 1 : 0;
  const shown = () => props.open || props.dragPx !== null;

  const translate = () => {
    const hidden = (1 - fraction()) * 100;
    return `translateX(${props.side === 'left' ? -hidden : hidden}%)`;
  };
  const transition = () =>
    props.dragPx !== null || prefersReducedMotion() ? 'none' : 'transform 200ms ease-out';

  createEffect(
    on(
      () => props.open,
      (open, wasOpen) => {
        if (open && !wasOpen) {
          opener = document.activeElement;
          const first = panel.querySelector<HTMLElement>(FOCUSABLE);
          (first ?? panel).focus();
          // Back closes the drawer. Only the back press consumes the entry.
          releaseBack = navStack().push(() => props.onOpenChange(false));
        } else if (!open && wasOpen) {
          // Closed some other way: take the history entry with it. A release
          // after back already ran is a no-op.
          releaseBack?.();
          releaseBack = null;
          if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
          opener = null;
        }
      },
    ),
  );
  onCleanup(() => releaseBack?.());

  // As an attribute, not the `inert` property: the attribute is what every
  // engine and every test DOM reads, and Solid's JSX has no attribute form.
  createEffect(() => panel.toggleAttribute('inert', !props.open));

  const onKeyDown = (e: KeyboardEvent) => {
    if (!props.open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      props.onOpenChange(false);
      return;
    }
    if (e.key !== 'Tab') return;
    const items = [...panel.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <>
      <div
        data-testid={`drawer-scrim-${props.side}`}
        class="fixed inset-0 z-40 bg-black/40"
        style={{
          opacity: fraction(),
          'pointer-events': shown() ? 'auto' : 'none',
          transition: props.dragPx !== null || prefersReducedMotion() ? 'none' : 'opacity 200ms ease-out',
        }}
        onClick={() => props.onOpenChange(false)}
        aria-hidden="true"
      />
      <div
        ref={panel}
        data-testid={`drawer-${props.side}`}
        role={props.open ? 'dialog' : undefined}
        aria-modal={props.open ? 'true' : undefined}
        aria-label={props.label}
        aria-hidden={props.open ? undefined : 'true'}
        tabIndex={-1}
        class={`fixed top-0 bottom-0 z-50 flex flex-col bg-surface-base border-hairline shadow-xl outline-none ${
          props.side === 'left' ? 'left-0 border-r' : 'right-0 border-l'
        }`}
        style={{
          width: `${props.width}px`,
          transform: translate(),
          transition: transition(),
          'padding-top': 'var(--inset-top)',
          'padding-bottom': 'var(--inset-bottom)',
        }}
        onKeyDown={onKeyDown}
      >
        {props.children}
      </div>
    </>
  );
};
