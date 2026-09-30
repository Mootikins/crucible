/** The tool-line handlers of a transcript, on the mock store. */
import { openReview } from '../actions';
import type { ToolLineHandlers } from '../components/session/types';
import { hunkLines, setState, state } from '../state';

export const mockToolHandlers: ToolLineHandlers = {
  isOpen: (id) => !!state.open[id],
  onToggle: (id) => setState('open', id, !state.open[id]),
  hunkFor: (hunkId) => {
    const h = state.hunks[hunkId];
    return h ? { state: h.state, path: h.path, ...hunkLines(hunkId) } : undefined;
  },
  onOpenPath: (path) => { const sid = Object.values(state.hunks).find(h => h.path === path)?.session ?? state.active; openReview(sid === 's3' ? 'proposal' : 'record', sid, path); },
};
