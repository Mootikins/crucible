/**
 * A hover popup with no bar, variant "none": pin, open in a tab and close
 * sit at the top-right corner. They show while the pointer is over the popup
 * and stay a moment after it leaves (the stylesheet delays the fade out).
 */
import type { Component } from 'solid-js';
import { ExternalLink, Pin, X } from '@/lib/icons';
import { IconButton } from '../primitives/IconButton';
import type { HoverActions } from './HoverTitleBar';

export const HoverCornerControls: Component<HoverActions & { onOpenInTab: () => void }> = (props) => (
  <div class="mk-hovercorner" data-wm-drag-handle="">
    <IconButton label="Pin: keep it open, with all its tools" onClick={() => props.onPin()}>
      <Pin class="mk-i" />
    </IconButton>
    <IconButton label="Open in a tab" onClick={() => props.onOpenInTab()}>
      <ExternalLink class="mk-i" />
    </IconButton>
    <IconButton label="Close" onClick={() => props.onClose()}>
      <X class="mk-i" />
    </IconButton>
  </div>
);
