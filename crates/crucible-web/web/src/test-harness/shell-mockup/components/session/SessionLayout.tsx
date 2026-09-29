/**
 * The frame of a session: the header, the transcript that scrolls, and the
 * composer area under it. `insetRight` keeps both columns clear of a window
 * that floats over the right edge.
 */
import type { Component, JSX } from 'solid-js';

export interface SessionLayoutProps {
  ref?: (el: HTMLDivElement) => void;
  insetRight: number;
  header: JSX.Element;
  transcript: JSX.Element;
  /** The permission card, the queue and the composer. */
  footer: JSX.Element;
}

export const SessionLayout: Component<SessionLayoutProps> = (props) => {
  const inset = () => (props.insetRight ? `${props.insetRight}px` : undefined);
  return (
    <div class="mk-session" ref={props.ref}>
      {props.header}
      <div class="mk-scroll mk-transcript" style={{ 'padding-right': inset() }}>
        {props.transcript}
      </div>
      <div class="mk-composerwrap" style={{ 'padding-right': inset() }}>
        <div class="mk-cinner">{props.footer}</div>
      </div>
    </div>
  );
};
