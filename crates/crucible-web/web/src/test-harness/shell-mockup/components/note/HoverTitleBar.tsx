/** A hover popup's bar, variant "title": the note's title, then pin and close. The bar drags the popup. */
import type { Component } from 'solid-js';
import { Pin, X } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';
import { scrollFade } from '../primitives/scrollFade';

export interface HoverActions {
  title: string;
  onPin: () => void;
  onClose: () => void;
}

export const HoverTitleBar: Component<HoverActions> = (props) => (
  <div class="mk-hoverbar" data-wm-drag-handle="">
    <span class="mk-hovertitle" ref={scrollFade('x')}>{props.title}</span>
    <IconButton label="Pin: keep it open, with all its tools" onClick={() => props.onPin()}>
      <Pin class="mk-i" />
    </IconButton>
    <IconButton label="Close" onClick={() => props.onClose()}>
      <X class="mk-i" />
    </IconButton>
  </div>
);
