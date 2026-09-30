/** Back and forward through a document tab's history, as in a web browser. */
import type { Component } from 'solid-js';
import { ChevronLeft, ChevronRight } from '@/lib/icons';
import { IconButton } from '../primitives/IconButton';

export interface HistoryNavProps {
  canBack: boolean;
  canForward: boolean;
  onGo: (step: -1 | 1) => void;
}

export const HistoryNav: Component<HistoryNavProps> = (props) => (
  <div class="mk-nav">
    <IconButton label="Back (Alt+Left)" disabled={!props.canBack} onClick={() => props.onGo(-1)}>
      <ChevronLeft class="mk-i" />
    </IconButton>
    <IconButton label="Forward (Alt+Right)" disabled={!props.canForward} onClick={() => props.onGo(1)}>
      <ChevronRight class="mk-i" />
    </IconButton>
  </div>
);
