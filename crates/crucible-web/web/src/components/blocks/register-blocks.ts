import { registerBlock } from './registry';
import { KanbanBlock } from './KanbanBlock';
import { GraphBlock } from './GraphBlock';

/**
 * Blocks that ship with the app.
 *
 * Two entries, and they are reference implementations rather than features.
 * Between them they cover both halves of the plugin contract's read path:
 * `kanban/board` draws data the plugin **published**, and
 * `graph/neighborhood` **invokes a command** for an answer that depends on
 * arguments only the browser knows. See `registry.ts` for why third-party
 * components cannot be loaded here yet.
 */
registerBlock('kanban', 'board', KanbanBlock);
registerBlock('graph', 'neighborhood', GraphBlock);
