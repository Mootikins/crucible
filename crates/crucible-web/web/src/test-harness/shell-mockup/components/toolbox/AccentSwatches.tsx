/** The accent colours, as a row of swatches; the first leaves the accent to the theme. */
import { For, type Component } from 'solid-js';
import { Swatch } from '../primitives/Swatch';
import type { AccentId, AccentOption } from './types';

export const AccentSwatches: Component<{ options: AccentOption[]; value: AccentId; onPick: (id: AccentId) => void }> = (props) => (
  <div class="mk-swatches">
    <For each={props.options}>
      {(o) => (
        <Swatch
          title={o.id === 'theme' ? 'Theme default' : o.id}
          color={o.color}
          on={props.value === o.id}
          auto={o.id === 'theme'}
          onPick={() => props.onPick(o.id)}
        />
      )}
    </For>
  </div>
);
