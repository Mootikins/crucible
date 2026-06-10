import { Component, JSX } from 'solid-js';

interface PanelHeaderProps {
  title: string;
  children?: JSX.Element;
  class?: string;
}

/**
 * Shared panel header component for consistent header styling.
 * Provides: padding, bottom border, title styling (small, semibold, uppercase, tracking).
 * Supports optional additional classes (e.g., shrink-0) and additional children.
 */
export const PanelHeader: Component<PanelHeaderProps> = (props) => (
  <div class={`p-3 border-b border-neutral-800 ${props.class || ''}`}>
    <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">
      {props.title}
    </h2>
    {props.children}
  </div>
);
