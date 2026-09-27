import type { Component } from 'solid-js';
import { BaseView } from '../bases/BaseView';
import type { BlockProps } from './registry';

/** Legacy embeds use the native saved base; no global publication is read. */
export const KanbanBlock: Component<BlockProps> = props => <BaseView
  filePath={typeof props.params.base === 'string' ? props.params.base : 'tickets.base'}
  kiln={typeof props.params.kiln === 'string' ? props.params.kiln : undefined}
  view="Board"
/>;
