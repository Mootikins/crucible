/**
 * The look toolbox: every knob of the shell's look, in one panel at the
 * bottom right. "Copy CSS" gives the current values as a stylesheet.
 */
import type { Component } from 'solid-js';
import { Portal } from 'solid-js/web';
import { X } from 'lucide-solid';
import { Button } from '../primitives/Button';
import { CopyButton } from '../primitives/CopyButton';
import { IconButton } from '../primitives/IconButton';
import { CardRows } from './CardRows';
import { SurfaceRows } from './SurfaceRows';
import { TextRows } from './TextRows';
import { VariantRows } from './VariantRows';
import { ToneRows } from './ToneRows';
import type { AccentOption, SetTweak, Tweaks } from './types';

export interface ToolboxPanelProps {
  tweaks: Tweaks;
  accents: AccentOption[];
  onSet: SetTweak;
  onReset: () => void;
  onClose: () => void;
  /** The current look as a stylesheet, for "Copy CSS". */
  css: () => string;
}

export const ToolboxPanel: Component<ToolboxPanelProps> = (props) => (
  <Portal>
    <div class="mk-toolbox" role="dialog" aria-label="Look" onKeyDown={(e) => e.key === 'Escape' && props.onClose()}>
      <div class="mk-tb-head">
        <span>Look</span>
        <IconButton label="Close" onClick={() => props.onClose()}>
          <X class="w-3.5 h-3.5" />
        </IconButton>
      </div>
      <div class="mk-tb-body">
        <SurfaceRows tweaks={props.tweaks} onSet={props.onSet} />
        <ToneRows tweaks={props.tweaks} onSet={props.onSet} accents={props.accents} />
        <CardRows tweaks={props.tweaks} onSet={props.onSet} />
        <TextRows tweaks={props.tweaks} onSet={props.onSet} />
        <VariantRows tweaks={props.tweaks} onSet={props.onSet} />
      </div>
      <div class="mk-tb-foot">
        <Button variant="ghost" onClick={() => props.onReset()}>
          Reset
        </Button>
        <CopyButton text={props.css} label="Copy CSS" />
      </div>
    </div>
  </Portal>
);
