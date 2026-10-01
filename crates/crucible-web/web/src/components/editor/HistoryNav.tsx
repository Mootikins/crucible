/** Back and forward through a document tab's history, as in a web browser. */
import type { Component } from 'solid-js';
import { ChevronLeft, ChevronRight } from '@/lib/icons';
import { IconButton } from '@/components/ui/IconButton';

export interface HistoryNavProps {
  canBack: boolean;
  canForward: boolean;
  onGo: (step: -1 | 1) => void;
}

export const HistoryNav: Component<HistoryNavProps> = (props) => (
  <div class="note-history flex items-center">
    <IconButton size="sm" aria-label="Back (Alt+Left)" disabled={!props.canBack} onClick={() => props.onGo(-1)}>
      <ChevronLeft class="w-3.5 h-3.5" />
    </IconButton>
    <IconButton size="sm" aria-label="Forward (Alt+Right)" disabled={!props.canForward} onClick={() => props.onGo(1)}>
      <ChevronRight class="w-3.5 h-3.5" />
    </IconButton>
  </div>
);
