import type { Component } from 'solid-js';
import { BaseView } from '../bases/BaseView';
import { baseViewProps } from '../bases/mount';
import type { BlockProps } from './registry';

/** The base that the kanban plugin creates when its setup names no other (`runtime/plugins/kanban`). */
const DEFAULT_TICKET_BASE = 'tickets.base';

const text = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);

/**
 * A legacy `kanban/board` block shows the native saved base. No global
 * publication is read. The note that shows the block names the kiln when the
 * block does not. With no `view` parameter the base shows its first view, so
 * a base without a "Board" view still shows.
 */
export const KanbanBlock: Component<BlockProps> = props => <BaseView {...baseViewProps({
  filePath: text(props.params.base) ?? DEFAULT_TICKET_BASE,
  view: text(props.params.view),
  host: props.host,
  kiln: () => text(props.params.kiln) ?? props.kiln,
})} />;
