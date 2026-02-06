import { type JSXElement, type Component, Show } from 'solid-js';
import { type Zone } from '@/lib/panel-registry';

export interface ZoneWrapperProps {
  zone: Zone;
  collapsed: boolean;
  width?: number;
  height?: number;
  id?: string;
  ref?: HTMLDivElement | ((el: HTMLDivElement) => void);
  children?: JSXElement;
  onTransitionEnd?: (event: TransitionEvent) => void;
}

export const ZoneWrapper: Component<ZoneWrapperProps> = (props) => {
  const isCenter = () => props.zone === 'center';
  const isBottom = () => props.zone === 'bottom';

  const style = (): Record<string, string> => {
    if (isCenter()) {
      return {
        flex: '1',
        'min-width': '0',
        overflow: 'hidden',
      };
    }

    if (isBottom()) {
      return {
        'flex-basis': props.collapsed ? '0px' : `${props.height ?? 200}px`,
        'flex-shrink': '0',
        'flex-grow': '0',
        overflow: 'hidden',
        transition: 'flex-basis 200ms ease-out',
      };
    }

    return {
      'flex-basis': props.collapsed ? '0px' : `${props.width ?? 280}px`,
      'flex-shrink': '0',
      'flex-grow': '0',
      overflow: 'hidden',
      opacity: props.collapsed ? '0' : '1',
      transition: 'flex-basis 200ms ease-out, opacity 150ms ease-out',
    };
  };

  return (
    <div
      id={props.id}
      data-zone={props.zone}
      data-testid={`zone-${props.zone}`}
      ref={props.ref}
      style={style()}
      onTransitionEnd={props.onTransitionEnd}
    >
      <Show when={!props.collapsed || isCenter()}>
        {props.children}
      </Show>
    </div>
  );
};
