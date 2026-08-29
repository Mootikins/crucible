import { createMemo, untrack } from 'solid-js';
import type { Tab } from '@/types/windowTypes';

/**
 * A tab's metadata as REACTIVE panel props.
 *
 * A pane deliberately re-renders its panel only when the active tab's IDENTITY
 * or content type changes: `updateTab` replaces the whole tabs array on every
 * write, so re-rendering on the tab object itself would remount the panel and,
 * for the editor, discard in-progress edits.
 *
 * Metadata used to be write-once, so an untracked snapshot was safe. It is not
 * any more — retargeting an open draft at another project rewrites `workspace`
 * on the SAME tab, and a plain spread froze that value at mount: the second
 * "New session in <project>" silently kept the first project.
 *
 * Getters give both halves. The panel stays mounted, and every metadata key it
 * reads tracks the live tab, so a later write reaches it. Solid's JSX spread
 * compiles to `mergeProps`, which preserves getters and reads them lazily.
 *
 * Each key sits behind its OWN memo, and that is load-bearing, not tidiness.
 * `updateTab` replaces the whole `tabs` array, so a bare getter would make
 * every panel prop depend on every write to any tab in the group — and the
 * editor writes `isModified` back on each keystroke. A panel that re-read a
 * prop and wrote in response then fed itself: the first version of this
 * function hung the app with "Maximum call stack size exceeded". A memo's
 * default `===` check stops an unchanged value at the boundary, so the churn
 * dies here instead of reaching the panel.
 *
 * The key SET is fixed when the panel mounts: a key ADDED to the metadata
 * later does not appear. Every caller writes its full key set at tab creation,
 * and reading `Object.keys` of a live store on every access would make the
 * whole props object a dependency — which is the remount this exists to avoid.
 */
export function reactiveMetadataProps(
  liveTab: () => Tab | null,
): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  // One argument, not the same tab twice: two parameters invited a caller to
  // pass a mismatched pair.
  for (const key of Object.keys(untrack(liveTab)?.metadata ?? {})) {
    const value = createMemo(
      () => (liveTab()?.metadata as Record<string, unknown> | undefined)?.[key],
    );
    Object.defineProperty(props, key, { enumerable: true, get: value });
  }
  return props;
}
