/** How long the model thought before it answered. */
import type { Component } from 'solid-js';
import { Brain } from '@/lib/icons';

export const ThinkingLine: Component<{ secs: number }> = (props) => (
  <div class="mk-thinking">
    <Brain class="mk-i" />
    Thought for {props.secs} s
  </div>
);
