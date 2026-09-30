/**
 * The look store: the toolbox's knobs, saved in localStorage, and the code
 * that writes them to the root element as a plugin theme would.
 */
import { createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import { windowActions } from '@/windowing/store';
import { setState } from './state';
import type { AccentId, AccentOption, EdgeId, PluginCssId, ShadowId, Tweaks } from './components/toolbox/types';

const ACCENTS: Record<AccentId, { dark: string; light: string } | null> = {
  theme: null,
  blue: { dark: '#6e9ef0', light: '#2e62c8' },
  violet: { dark: '#a58cf0', light: '#6a4fb0' },
  teal: { dark: '#5fc4bc', light: '#1f7a74' },
  amber: { dark: '#e0b04a', light: '#8a6200' },
  graphite: { dark: '#c5c6cc', light: '#34363d' },
};

const SHADOWS: Record<ShadowId, { dark: string; light: string }> = {
  none: { dark: 'none', light: 'none' },
  soft: { dark: '0 18px 40px -24px rgba(0, 0, 0, 0.75)', light: '0 18px 40px -24px rgba(20, 22, 28, 0.3)' },
  deep: { dark: '0 24px 60px -18px rgba(0, 0, 0, 0.9)', light: '0 24px 60px -18px rgba(20, 22, 28, 0.45)' },
};

const EDGES: Record<EdgeId, string> = {
  none: 'transparent',
  hairline: 'var(--cru-color-hairline)',
  strong: 'var(--cru-color-hairline-strong)',
};

const PLUGIN_CSS: Record<PluginCssId, string> = {
  none: '',
  docs: `:root { --cru-color-primary:#3a7fe0; --cru-color-primary-hover:#5e9bf0; --cru-color-primary-active:#2e63b4; --cru-color-on-primary:#0b0f17; --cru-radius-control:0px; --cru-radius-card:2px; --cru-radius-composer:4px; }
:root[data-theme='light'] { --cru-color-primary:#1f4a91; --cru-color-primary-hover:#17376d; --cru-color-primary-active:#102850; --cru-color-on-primary:#ffffff; }`,
};

const defaults = (): Tweaks => ({
  theme: 'dark',
  // Glass, except where the OS asks for less transparency.
  material: 'flat',
  tabs: 'leaf',
  railTabs: 'vertical',
  darkPanes: true,
  gap: 8,
  radius: 12,
  edges: 'none',
  lines: false,
  contrast: 8,
  tint: 0,
  black: false,
  shadow: 'none',
  grain: 4,
  accent: 'theme',
  reading: 15,
  plugin: 'none',
  changesControls: 'ab',
});

// v2: the defaults changed, so the settings saved under v1 no longer apply.
const KEY = 'crucible-shell-mockup-tweaks-v2';
function load(): Partial<Tweaks> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Tweaks>;
  } catch {
    return {};
  }
}

const [tweaks, setTweaks] = createStore<Tweaks>({ ...defaults(), ...load() });
export { tweaks };

/** The accent choices. The colour follows the theme, so each option reads it when asked. */
export const accentOptions: AccentOption[] = (Object.keys(ACCENTS) as AccentId[]).map((id) => ({
  id,
  get color() {
    return ACCENTS[id]?.[tweaks.theme];
  },
}));

/** The variables the knobs write, as one map: `applyTweaks` and "Copy CSS" share it. */
function variables(t: Tweaks): Record<string, string> {
  const vars: Record<string, string> = {
    '--mk-gap': `${t.gap}px`,
    '--mk-radius': `${t.radius}px`,
    '--mk-edge': EDGES[t.edges],
    '--mk-contrast': String(t.contrast),
    '--mk-nav-tint': String(t.tint),
    '--mk-shadow': SHADOWS[t.shadow][t.theme],
    '--mk-grain-opacity': String(t.grain / 100),
    '--mk-font-note': `${t.reading / 16}rem`,
  };
  const accent = ACCENTS[t.accent];
  if (accent) {
    const c = accent[t.theme];
    vars['--cru-color-primary'] = c;
    vars['--cru-color-primary-hover'] = `color-mix(in oklab, ${c}, ${t.theme === 'dark' ? 'white' : 'black'} 15%)`;
    vars['--cru-color-primary-active'] = `color-mix(in oklab, ${c}, ${t.theme === 'dark' ? 'black' : 'white'} 12%)`;
    vars['--cru-color-on-primary'] = t.theme === 'dark' ? '#0d1017' : '#ffffff';
  }
  return vars;
}

/** The current look as a stylesheet, for "Copy CSS". */
export function tweaksCss(): string {
  const t = tweaks;
  const lines = Object.entries(variables(t)).map(([k, v]) => `  ${k}: ${v};`);
  return `/* Shell look: ${t.theme}, ${t.material}, ${t.lines ? ', inner lines' : ''}. */\n:root {\n${lines.join('\n')}\n}\n`;
}

const ACCENT_VARS = ['--cru-color-primary', '--cru-color-primary-hover', '--cru-color-primary-active', '--cru-color-on-primary'];

export function applyTweaks() {
  const t = tweaks;
  const root = document.documentElement;
  // The app's rule: dark writes no attribute.
  if (t.theme === 'light') root.setAttribute('data-theme', 'light');
  else root.removeAttribute('data-theme');
  setState('theme', t.theme);
  root.dataset.material = t.material;
  root.toggleAttribute('data-mk-lines', t.lines);
  root.toggleAttribute('data-mk-black', t.black);
  // `pill` is the windowing library's default theme; `flat` and `leaf` restyle it.
  root.dataset.mkTabs = t.tabs;
  root.dataset.mkRailtabs = t.railTabs;
  root.toggleAttribute('data-mk-darkpanes', t.darkPanes);
  // The rail icons sit at the window edge. The inside placement is off.
  windowActions.setRibbonPlacement('edge');
  // Floating windows have no title bar: their controls sit in the tab bar, or
  // in the note's own bar when the window shows one note.
  windowActions.setFloatingChrome('merged');
  for (const v of ACCENT_VARS) root.style.removeProperty(v);
  for (const [k, v] of Object.entries(variables(t))) root.style.setProperty(k, v);
  let el = document.getElementById('mk-plugin-theme');
  if (!el) {
    el = document.createElement('style');
    el.id = 'mk-plugin-theme';
    document.head.append(el);
  }
  el.textContent = PLUGIN_CSS[t.plugin];
  // Store only what differs from the defaults, so a later change of a
  // default reaches a browser that opened the page before.
  const d = defaults();
  const changed = Object.fromEntries(Object.entries(t).filter(([k, v]) => d[k as keyof Tweaks] !== v));
  localStorage.setItem(KEY, JSON.stringify(changed));
}

export function setTweak<K extends keyof Tweaks>(key: K, value: Tweaks[K]) {
  setTweaks(key, value as never);
  applyTweaks();
}

export function resetTweaks() {
  setTweaks(defaults());
  applyTweaks();
}

const [toolboxOpen, setToolboxOpen] = createSignal(false);
export { toolboxOpen, setToolboxOpen };
export const toggleToolbox = () => setToolboxOpen((o) => !o);
