import { Component, JSX } from 'solid-js';
import type { SetStoreFunction } from 'solid-js/store';
import { DEFAULT_GRAPH_SETTINGS, type GraphSettings } from '@/lib/graph/types';

const Section: Component<{ title: string; children: JSX.Element }> = (props) => (
  <div class="flex flex-col gap-1.5">
    <div class="text-[10px] uppercase tracking-wider text-muted-dark">{props.title}</div>
    {props.children}
  </div>
);

const Slider: Component<{
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onInput: (v: number) => void;
}> = (props) => (
  <label class="flex flex-col gap-0.5">
    <span class="flex justify-between text-muted">
      <span>{props.label}</span>
      <span class="tabular-nums text-muted-dark">{props.value}</span>
    </span>
    <input
      type="range"
      class="w-full h-1 accent-primary cursor-pointer"
      min={props.min}
      max={props.max}
      step={props.step}
      value={props.value}
      onInput={(e) => props.onInput(Number(e.currentTarget.value))}
    />
  </label>
);

const Toggle: Component<{
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}> = (props) => (
  <label class="flex items-center justify-between gap-2 text-muted cursor-pointer">
    <span>{props.label}</span>
    <input
      type="checkbox"
      class="accent-primary cursor-pointer"
      checked={props.checked}
      onChange={(e) => props.onChange(e.currentTarget.checked)}
    />
  </label>
);

/** Obsidian-style graph settings card (filters / display / forces). */
export const GraphControls: Component<{
  settings: GraphSettings;
  onChange: SetStoreFunction<GraphSettings>;
}> = (props) => {
  return (
    <div class="absolute top-11 right-2 w-60 max-h-[calc(100%-4rem)] overflow-y-auto flex flex-col gap-3 rounded-md border border-hairline bg-surface-elevated p-3 text-xs text-shell-body shadow-lg">
      <Section title="Filters">
        <input
          type="search"
          placeholder="Search notes…"
          class="w-full px-2 py-1 rounded border border-hairline bg-surface-base text-shell-body placeholder:text-muted-dark outline-none focus:border-primary"
          value={props.settings.filters.query}
          onInput={(e) => props.onChange('filters', 'query', e.currentTarget.value)}
        />
        <Toggle
          label="Tags"
          checked={props.settings.filters.showTags}
          onChange={(v) => props.onChange('filters', 'showTags', v)}
        />
        <Toggle
          label="Unresolved links"
          checked={props.settings.filters.showPhantoms}
          onChange={(v) => props.onChange('filters', 'showPhantoms', v)}
        />
        <Toggle
          label="Orphans"
          checked={props.settings.filters.showOrphans}
          onChange={(v) => props.onChange('filters', 'showOrphans', v)}
        />
      </Section>

      <Section title="Display">
        <Slider
          label="Node size"
          min={0.4}
          max={2.5}
          step={0.05}
          value={props.settings.display.nodeSize}
          onInput={(v) => props.onChange('display', 'nodeSize', v)}
        />
        <Slider
          label="Link thickness"
          min={0.3}
          max={3}
          step={0.05}
          value={props.settings.display.linkThickness}
          onInput={(v) => props.onChange('display', 'linkThickness', v)}
        />
      </Section>

      <Section title="Forces">
        <Slider
          label="Center force"
          min={0}
          max={1}
          step={0.05}
          value={props.settings.forces.centerForce}
          onInput={(v) => props.onChange('forces', 'centerForce', v)}
        />
        <Slider
          label="Repel force"
          min={0}
          max={2}
          step={0.05}
          value={props.settings.forces.repelForce}
          onInput={(v) => props.onChange('forces', 'repelForce', v)}
        />
        <Slider
          label="Link force"
          min={0}
          max={1}
          step={0.05}
          value={props.settings.forces.linkForce}
          onInput={(v) => props.onChange('forces', 'linkForce', v)}
        />
        <Slider
          label="Link distance"
          min={30}
          max={500}
          step={5}
          value={props.settings.forces.linkDistance}
          onInput={(v) => props.onChange('forces', 'linkDistance', v)}
        />
      </Section>

      <button
        type="button"
        class="self-start px-2 py-1 rounded border border-hairline text-muted hover:bg-hover-wash hover:text-shell-ink transition-colors"
        onClick={() => {
          const d = structuredClone(DEFAULT_GRAPH_SETTINGS);
          props.onChange('filters', d.filters);
          props.onChange('display', d.display);
          props.onChange('forces', d.forces);
        }}
      >
        Reset to defaults
      </button>
    </div>
  );
};
