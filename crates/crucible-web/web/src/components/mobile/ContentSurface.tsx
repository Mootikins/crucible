import { Component, JSX, createMemo } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { getGlobalRegistry } from '@/lib/panel-registry';
import { reactiveMetadataProps } from '@/lib/panel-props';
import type { Tab } from '@/types/windowTypes';

/**
 * The compact shell's one content area: the registered panel for one tab.
 *
 * It renders the way `windowing/Pane.tsx` does, and for the same reason. A tab
 * object changes on every `isModified` write, because `updateTab` replaces it.
 * A panel keyed on that object would remount on each keystroke and drop the
 * editor's buffer. So the panel is keyed on the tab's identity and content type
 * alone, and its metadata arrives through `reactiveMetadataProps`, whose per-key
 * memos deliver a later write without a remount.
 */
export const ContentSurface: Component<{
  tab: () => Tab | null;
  /** What the surface shows with no tab open. */
  empty: JSX.Element;
}> = (props) => {
  const tabId = createMemo(() => props.tab()?.id ?? null);
  const contentType = createMemo(() => props.tab()?.contentType ?? null);

  // Keyed on identity: a new id or type rebuilds the panel, and nothing else does.
  const panel = createMemo(() => {
    const id = tabId();
    const type = contentType();
    if (!id || !type) return null;
    const def = getGlobalRegistry().get(type);
    if (!def) return 'unknown' as const;
    const panelProps = reactiveMetadataProps(props.tab);
    return <Dynamic component={def.component} {...panelProps} />;
  });

  // Read inside JSX braces so the surface re-reads `panel()` when it changes.
  // A `<Show>` callback would run once and keep drawing the first panel.
  const view = () => {
    const content = panel();
    if (content === null) return props.empty;
    if (content === 'unknown') {
      return (
        <div class="flex-1 flex items-center justify-center">
          <p class="text-muted-dark text-sm">Unknown content type</p>
        </div>
      );
    }
    return content;
  };

  return <div class="flex-1 min-h-0 flex flex-col overflow-hidden bg-shell-bg">{view()}</div>;
};
