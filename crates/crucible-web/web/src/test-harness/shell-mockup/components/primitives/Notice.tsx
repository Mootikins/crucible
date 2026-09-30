/**
 * A short, quiet notice beside a control, for a change that shows nothing
 * else. It fades out by itself (`.mk-notice`); the caller removes it after.
 */
import type { Component } from 'solid-js';
import { Portal } from 'solid-js/web';

export const Notice: Component<{ text: string; anchor: DOMRect }> = (props) => (
  <Portal>
    <div
      class="mk-notice"
      role="status"
      style={{ left: `${props.anchor.right + 8}px`, top: `${props.anchor.top + props.anchor.height / 2}px` }}
    >
      {props.text}
    </div>
  </Portal>
);
