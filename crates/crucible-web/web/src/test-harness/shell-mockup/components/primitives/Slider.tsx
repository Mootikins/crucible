/** A range input with its value, and an optional unit, on the right. */
import type { Component } from 'solid-js';

export interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  onInput: (v: number) => void;
}

export const Slider: Component<SliderProps> = (props) => (
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
