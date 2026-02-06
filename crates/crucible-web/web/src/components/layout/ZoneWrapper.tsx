import { type JSXElement, type Component, Show } from 'solid-js';

export interface ZoneWrapperProps {
  zone: string;
  collapsed: boolean;
  width?: number;
  height?: number;
  ref?: HTMLDivElement | ((el: HTMLDivElement) => void);
  children?: JSXElement;
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

    if (props.collapsed) {
      return {
        'flex-basis': '0px',
        'flex-shrink': '0',
        'flex-grow': '0',
        overflow: 'hidden',
      };
    }

    if (isBottom()) {
      return {
        'flex-basis': `${props.height ?? 200}px`,
        'flex-shrink': '0',
        'flex-grow': '0',
        overflow: 'hidden',
      };
    }

    return {
      'flex-basis': `${props.width ?? 280}px`,
      'flex-shrink': '0',
      'flex-grow': '0',
      overflow: 'hidden',
    };
  };

  return (
    <div
      data-zone={props.zone}
      data-testid={`zone-${props.zone}`}
      ref={props.ref}
      style={style()}
    >
      <Show when={!props.collapsed || isCenter()}>
        {props.children}
      </Show>
    </div>
  );
};
