import type { Component } from 'solid-js';

/**
 * The classes of every ribbon button. The rail ribbon, the pane strip and the
 * app's rail chrome use them, so all the buttons on a rail match.
 *
 * The part class `wm-ribbon-btn` carries the look. `windowing/theme.css`
 * styles it; the other classes lay the icon out.
 */
export const ribbonBtn = 'wm-ribbon-btn flex items-center justify-center';

/** One command button on the ribbon (opens a modal/panel — Obsidian puts
 * these on the ribbon: palette, quick actions, settings gear at bottom). */
export const RibbonCommand: Component<{
  title: string;
  testId: string;
  onClick: () => void;
  children: ReturnType<Component>;
}> = (props) => (
  <button
    type="button"
    data-testid={props.testId}
    class={`${ribbonBtn} wm-ribbon-cmd w-10 flex-none`}
    title={props.title}
    onClick={() => props.onClick()}
  >
    {props.children}
  </button>
);
