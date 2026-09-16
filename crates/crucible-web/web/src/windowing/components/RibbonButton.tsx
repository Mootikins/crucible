import type { Component } from 'solid-js';

/**
 * The look of every ribbon button. The rail ribbon, the pane strip and the
 * app's rail chrome use it, so all the buttons on a rail match.
 */
export const ribbonBtn =
  'flex items-center justify-center text-muted-dark hover:text-shell-body hover:bg-hover-wash transition-colors';

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
    class={`${ribbonBtn} w-10 h-9 flex-none`}
    title={props.title}
    onClick={() => props.onClick()}
  >
    {props.children}
  </button>
);
