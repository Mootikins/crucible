import { Component, JSX } from 'solid-js';

interface PanelHeaderProps {
  title: string;
  children?: JSX.Element;
  class?: string;
}

/**
 * Shared panel header component for consistent header styling.
 * Uses the same quiet header and spacing as the navigation panes.
 * Supports optional additional classes (e.g., shrink-0) and additional children.
 */
export const PanelHeader: Component<PanelHeaderProps> = (props) => (
  <div class={`panel-header ${props.class || ''}`}>
    <h2 class="panel-title">
      {props.title}
    </h2>
    {props.children}
  </div>
);
