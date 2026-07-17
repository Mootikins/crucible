import { Component, JSX } from 'solid-js';

interface PanelShellProps {
  children: JSX.Element;
  class?: string;
}

/**
 * Shared panel shell component for consistent panel styling.
 * Provides base layout: full height, flex column, dark background, light text.
 * Supports optional additional classes for overflow/positioning variants.
 */
export const PanelShell: Component<PanelShellProps> = (props) => (
  <div class={`h-full flex flex-col bg-shell-panel text-shell-ink ${props.class || ''}`}>
    {props.children}
  </div>
);
