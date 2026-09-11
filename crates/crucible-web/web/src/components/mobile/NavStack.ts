/**
 * The hardware back button, as a stack of layers.
 *
 * A phone's back button (and Android's back gesture) fires `popstate`. The
 * compact shell gives each closable layer — a drawer, a sheet — one history
 * entry, and closes layers when back arrives.
 *
 * Each entry carries an id, and popstate closes every layer ABOVE the entry the
 * browser landed on. It is derived from the landing, not counted: a counter of
 * self-caused popstates drifts the first time a `history.back()` yields no
 * event, and then it swallows a real back press.
 *
 * Two rules come from the design note, section 6:
 * - `pushState` gets NO url argument. Passing `location.pathname` would strip
 *   the hash, and deep links live in the hash (`/#note=…`, decision log
 *   2026-08-13). With no argument the whole url stays as it was.
 * - A layer that closes some other way — a scrim tap, a selection — takes its
 *   entry with it, or the next back press would close a layer already gone.
 */

type BackHandler = () => void;

interface Layer {
  id: number;
  onBack: BackHandler;
}

export interface NavStack {
  /** Add a layer. Returns `release`, to call when the layer closes by itself. */
  push(onBack: BackHandler): () => void;
  dispose(): void;
}

const idOf = (state: unknown): number => {
  const id = (state as { crucibleNav?: unknown } | null)?.crucibleNav;
  return typeof id === 'number' ? id : 0;
};

export function createNavStack(win: Window = window): NavStack {
  const layers: Layer[] = [];
  // Start above any id a reload left in this tab's history, so an old entry
  // can never share an id with a new layer.
  let seq = idOf(win.history.state);

  const onPopState = (e: Event) => {
    const landed = idOf((e as PopStateEvent).state);
    // Close top-down every layer above the landing. A release already removed
    // its own layer, so the popstate its history.back() causes closes nothing.
    while (layers.length > 0 && layers[layers.length - 1].id > landed) {
      layers.pop()!.onBack();
    }
  };
  win.addEventListener('popstate', onPopState);

  return {
    push(onBack) {
      const layer: Layer = { id: ++seq, onBack };
      layers.push(layer);
      win.history.pushState({ crucibleNav: layer.id }, '');
      return () => {
        const at = layers.indexOf(layer);
        // Already gone: back ran it, so its entry is already consumed.
        if (at === -1) return;
        layers.splice(at, 1);
        win.history.back();
      };
    },
    dispose() {
      win.removeEventListener('popstate', onPopState);
      layers.length = 0;
    },
  };
}

/** The shell's one stack. Created on first use, so importing costs nothing. */
let shared: NavStack | null = null;
export function navStack(): NavStack {
  shared ??= createNavStack();
  return shared;
}
