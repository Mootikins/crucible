/** The tool-line handlers of a transcript, on the mock store. */
import { openNote } from '../actions';
import type { ToolLineHandlers } from '../components/session/types';
import { decide, hunkLines, setState, state } from '../state';

export const mockToolHandlers: ToolLineHandlers = {
  isOpen: (id) => !!state.open[id],
  onToggle: (id) => setState('open', id, !state.open[id]),
  hunkFor: (hunkId) => {
    const h = state.hunks[hunkId];
    return h ? { state: h.state, path: h.path, ...hunkLines(hunkId) } : undefined;
  },
  onOpenPath: (path) => openNote(path, { fromSession: true }),
  onDecide: (hunkId, accept) => decide([hunkId], accept),
};
