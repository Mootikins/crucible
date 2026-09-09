import { registerBlock } from './registry';
import { KanbanBlock } from './KanbanBlock';

/**
 * Blocks that ship with the app.
 *
 * One entry today, and it is a reference implementation rather than a feature:
 * it demonstrates what a plugin's own web component looks like against the
 * publication contract. See `registry.ts` for why third-party components
 * cannot be loaded here yet.
 */
registerBlock('kanban', 'board', KanbanBlock);
