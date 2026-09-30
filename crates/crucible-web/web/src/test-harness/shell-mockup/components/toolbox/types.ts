/**
 * The knobs of the shell's look. Each knob writes a CSS variable or a root
 * attribute, as a plugin theme would, so this type doubles as a list of the
 * tokens that a real theme contract needs.
 */
export type AccentId = 'theme' | 'blue' | 'violet' | 'teal' | 'amber' | 'graphite';
export type ShadowId = 'none' | 'soft' | 'deep';
export type EdgeId = 'none' | 'hairline' | 'strong';
export type PluginCssId = 'none' | 'docs';

export interface Tweaks {
  theme: 'dark' | 'light';
  material: 'flat' | 'glass';
  tabs: 'flat' | 'leaf' | 'pill';
  railTabs: 'vertical' | 'horizontal';
  darkPanes: boolean;
  gap: number;
  radius: number;
  edges: EdgeId;
  lines: boolean;
  contrast: number;
  tint: number;
  black: boolean;
  shadow: ShadowId;
  grain: number;
  accent: AccentId;
  reading: number;
  plugin: PluginCssId;
  /** A/B variant: the changes view as B, or B with A's controls. */
  changesControls: 'b' | 'ab';
}

export type SetTweak = <K extends keyof Tweaks>(key: K, value: Tweaks[K]) => void;

/** The rows of the toolbox all take the current look and the setter. */
export interface TweakRowsProps {
  tweaks: Tweaks;
  onSet: SetTweak;
}

/** One accent choice. `theme` has no colour: the theme decides. */
export interface AccentOption {
  id: AccentId;
  color?: string;
}
