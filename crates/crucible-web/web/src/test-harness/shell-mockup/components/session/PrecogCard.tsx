/** The notes that precognition recalled for the turn, folded; each opens its note. */
import { For, type Component } from 'solid-js';
import { Sparkles } from 'lucide-solid';
import { basename } from '../path';

export const PrecogCard: Component<{ notes: [string, number][]; onOpen: (path: string) => void }> = (props) => (
  <details class="mk-precog">
    <summary>
      <Sparkles class="mk-i" />
      {props.notes.length} notes recalled
    </summary>
    <ul>
      <For each={props.notes}>
        {([p, score]) => (
          <li>
            <button type="button" class="mk-link" onClick={() => props.onOpen(p)}>
              {basename(p)}
            </button>
            <span class="mk-score">{score.toFixed(2)}</span>
          </li>
        )}
      </For>
    </ul>
  </details>
);
