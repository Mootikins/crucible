/**
 * The look toolbox: every knob of the shell's look, in one panel.
 *
 * Each knob writes a CSS variable or an attribute on the root element, as a
 * plugin theme would, so the panel doubles as a list of the tokens a real
 * theme contract needs. "Copy CSS" gives the current values as a stylesheet.
 */
import { For, Show, createSignal, type Component, type JSX } from 'solid-js';
import { createStore } from 'solid-js/store';
import { Portal } from 'solid-js/web';
import { X } from 'lucide-solid';
import { windowActions } from '@/windowing/store';
import { setState } from './state';

const ACCENTS = {
  theme: null,
  blue: { dark: '#6e9ef0', light: '#2e62c8' },
  violet: { dark: '#a58cf0', light: '#6a4fb0' },
  teal: { dark: '#5fc4bc', light: '#1f7a74' },
  amber: { dark: '#e0b04a', light: '#8a6200' },
  graphite: { dark: '#c5c6cc', light: '#34363d' },
} as const;
type AccentId = keyof typeof ACCENTS;

const SHADOWS = {
  none: { dark: 'none', light: 'none' },
  soft: { dark: '0 18px 40px -24px rgba(0, 0, 0, 0.75)', light: '0 18px 40px -24px rgba(20, 22, 28, 0.3)' },
  deep: { dark: '0 24px 60px -18px rgba(0, 0, 0, 0.9)', light: '0 24px 60px -18px rgba(20, 22, 28, 0.45)' },
} as const;

const EDGES = {
  none: 'transparent',
  hairline: 'var(--cru-color-hairline)',
  strong: 'var(--cru-color-hairline-strong)',
} as const;

const PLUGIN_CSS = {
  none: '',
  docs: `:root { --cru-color-primary:#3a7fe0; --cru-color-primary-hover:#5e9bf0; --cru-color-primary-active:#2e63b4; --cru-color-on-primary:#0b0f17; --cru-radius-control:0px; --cru-radius-card:2px; --cru-radius-composer:4px; }
:root[data-theme='light'] { --cru-color-primary:#1f4a91; --cru-color-primary-hover:#17376d; --cru-color-primary-active:#102850; --cru-color-on-primary:#ffffff; }`,
} as const;

interface Tweaks {
  theme: 'dark' | 'light';
  material: 'flat' | 'glass';
  tabs: 'flat' | 'leaf' | 'pill';
  railTabs: 'vertical' | 'horizontal';
  darkPanes: boolean;
  gap: number;
  radius: number;
  edges: keyof typeof EDGES;
  lines: boolean;
  contrast: number;
  tint: number;
  black: boolean;
  shadow: keyof typeof SHADOWS;
  grain: number;
  accent: AccentId;
  reading: number;
  plugin: keyof typeof PLUGIN_CSS;
}

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

function resetTweaks() {
  setTweaks(defaults());
  applyTweaks();
}

const [open, setOpen] = createSignal(false);
export const toggleToolbox = () => setOpen((o) => !o);

const Row: Component<{ label: string; hint?: string; children: JSX.Element }> = (props) => (
  <div class="mk-tb-row">
    <div class="mk-tb-label">
      {props.label}
      <Show when={props.hint}><small>{props.hint}</small></Show>
    </div>
    {props.children}
  </div>
);

function Segmented<V extends string>(props: {
  value: V;
  options: readonly (readonly [V, string])[];
  onChange: (v: V) => void;
}) {
  return (
    <div class="mk-tb-seg" role="radiogroup">
      <For each={props.options}>
        {([id, label]) => (
          <button
            type="button"
            role="radio"
            aria-checked={props.value === id}
            classList={{ on: props.value === id }}
            onClick={() => props.onChange(id)}
          >
            {label}
          </button>
        )}
      </For>
    </div>
  );
}

const Slider: Component<{ value: number; min: number; max: number; step?: number; unit?: string; onInput: (v: number) => void }> = (
  props,
) => (
  <div class="mk-slider">
    <input
      type="range"
      min={props.min}
      max={props.max}
      step={props.step ?? 1}
      value={props.value}
      onInput={(e) => props.onInput(Number(e.currentTarget.value))}
    />
    <span>
      {props.value}
      {props.unit ?? ''}
    </span>
  </div>
);

export const Toolbox: Component = () => {
  const [copied, setCopied] = createSignal(false);
  const copyCss = async () => {
    const t = tweaks;
    const lines = Object.entries(variables(t)).map(([k, v]) => `  ${k}: ${v};`);
    const css = `/* Shell look: ${t.theme}, ${t.material}, ${t.lines ? ', inner lines' : ''}. */\n:root {\n${lines.join('\n')}\n}\n`;
    await navigator.clipboard?.writeText(css);
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };
  return (
    <Show when={open()}>
      <Portal>
        <div class="mk-toolbox" role="dialog" aria-label="Look" onKeyDown={(e) => e.key === 'Escape' && setOpen(false)}>
          <div class="mk-tb-head">
            <span>Look</span>
            <button type="button" class="mk-iconbtn" title="Close" onClick={() => setOpen(false)}>
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
          <div class="mk-tb-body">
            <Row label="Theme">
              <Segmented value={tweaks.theme} options={[['dark', 'Dark'], ['light', 'Light']]} onChange={(v) => setTweak('theme', v)} />
            </Row>
            <Row label="Surface">
              <Segmented value={tweaks.material} options={[['flat', 'Flat'], ['glass', 'Glass']]} onChange={(v) => setTweak('material', v)} />
            </Row>
            <Row label="Centre tabs" hint="Leaf: the same cutout as the rail tabs">
              <Segmented value={tweaks.tabs} options={[['leaf', 'Leaf'], ['flat', 'Flat'], ['pill', 'Pill']]} onChange={(v) => setTweak('tabs', v)} />
            </Row>
            <Row label="Rail tabs" hint="Vertical: the ribbon icons only">
              <Segmented value={tweaks.railTabs} options={[['vertical', 'Vertical'], ['horizontal', 'Horizontal']]} onChange={(v) => setTweak('railTabs', v)} />
            </Row>
            <Row label="Right rail" hint="Card: the focus tone, padded">
              <Segmented
                value={tweaks.darkPanes ? 'dark' : 'ground'}
                options={[['dark', 'Card'], ['ground', 'Ground']]}
                onChange={(v) => setTweak('darkPanes', v === 'dark')}
              />
            </Row>
            <Row label="Contrast" hint="Nav against main">
              <Slider value={tweaks.contrast} min={0} max={14} onInput={(v) => setTweak('contrast', v)} />
            </Row>
            <Show when={tweaks.theme === 'dark'}>
              <Row label="True black" hint="The focus cards are #000 (OLED)">
                <Segmented
                  value={tweaks.black ? 'on' : 'off'}
                  options={[['off', 'Off'], ['on', 'On']]}
                  onChange={(v) => setTweak('black', v === 'on')}
                />
              </Row>
            </Show>
            <Row label="Nav tint" hint="Accent in the nav tone">
              <Slider value={tweaks.tint} min={0} max={20} unit="%" onInput={(v) => setTweak('tint', v)} />
            </Row>
            <Row label="Accent">
              <div class="mk-swatches">
                <For each={Object.keys(ACCENTS) as AccentId[]}>
                  {(id) => (
                    <button
                      type="button"
                      title={id === 'theme' ? 'Theme default' : id}
                      aria-pressed={tweaks.accent === id}
                      classList={{ 'mk-swatch': true, on: tweaks.accent === id, auto: id === 'theme' }}
                      style={{ background: ACCENTS[id]?.[tweaks.theme] }}
                      onClick={() => setTweak('accent', id)}
                    />
                  )}
                </For>
              </div>
            </Row>
            <Row label="Gap">
              <Slider value={tweaks.gap} min={0} max={16} unit="px" onInput={(v) => setTweak('gap', v)} />
            </Row>
            <Row label="Radius">
              <Slider value={tweaks.radius} min={0} max={20} unit="px" onInput={(v) => setTweak('radius', v)} />
            </Row>
            <Row label="Card edge">
              <Segmented
                value={tweaks.edges}
                options={[['none', 'None'], ['hairline', 'Hairline'], ['strong', 'Strong']]}
                onChange={(v) => setTweak('edges', v)}
              />
            </Row>
            <Row label="Inner lines" hint="Tab bars, headers, rail edges">
              <Segmented
                value={tweaks.lines ? 'on' : 'off'}
                options={[['off', 'Off'], ['on', 'On']]}
                onChange={(v) => setTweak('lines', v === 'on')}
              />
            </Row>
            <Row label="Shadow">
              <Segmented
                value={tweaks.shadow}
                options={[['none', 'None'], ['soft', 'Soft'], ['deep', 'Deep']]}
                onChange={(v) => setTweak('shadow', v)}
              />
            </Row>
            <Show when={tweaks.material === 'glass'}>
              <Row label="Grain">
                <Slider value={tweaks.grain} min={0} max={10} onInput={(v) => setTweak('grain', v)} />
              </Row>
            </Show>
            <Row label="Note text">
              <Slider value={tweaks.reading} min={13} max={19} unit="px" onInput={(v) => setTweak('reading', v)} />
            </Row>
            <Row label="Plugin CSS">
              <Segmented value={tweaks.plugin} options={[['none', 'None'], ['docs', 'Docs example']]} onChange={(v) => setTweak('plugin', v)} />
            </Row>
          </div>
          <div class="mk-tb-foot">
            <button type="button" class="mk-btn sm ghost" onClick={resetTweaks}>Reset</button>
            <button type="button" class="mk-btn sm" onClick={() => void copyCss()}>{copied() ? 'Copied' : 'Copy CSS'}</button>
          </div>
        </div>
      </Portal>
    </Show>
  );
};
