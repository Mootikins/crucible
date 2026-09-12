import { Component, For, Show, createSignal } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { X } from '@/lib/icons';
import { iconForContentType } from '@/lib/tab-icons';
import type { Tab } from '@/types/windowTypes';

/**
 * Every open tab as a card list — the phone browser's pattern, because a
 * phone draws one tab at a time and a strip of them would be unreadable.
 *
 * Most recent first: the list is read top-down, and the tab a user wants next
 * is nearly always the one they just left.
 */
export const TabOverview: Component<{
  tabs: Tab[];
  activeId: string | null;
  onPick: (tabId: string) => void;
  onClose: (tabId: string) => void;
}> = (props) => {
  const ordered = () => [...props.tabs].reverse();
  // Which tab asked about discarding, if any. A phone cannot rely on
  // window.confirm: an installed PWA may suppress it, and a suppressed confirm
  // reads as a broken button.
  const [asking, setAsking] = createSignal<string | null>(null);

  const requestClose = (tab: Tab) => {
    if (tab.isModified) {
      setAsking(tab.id);
      return;
    }
    props.onClose(tab.id);
  };

  return (
    <div class="flex-1 min-h-0 overflow-y-auto">
      <Show
        when={props.tabs.length > 0}
        fallback={
          <div class="flex-1 flex items-center justify-center p-8">
            <p class="text-reading text-muted-dark">No tabs are open.</p>
          </div>
        }
      >
        <ul class="flex flex-col gap-1 p-2">
          <For each={ordered()}>
            {(tab) => (
              <li class="flex flex-col gap-1">
                <Show when={asking() === tab.id}>
                  <div class="flex items-center gap-2 h-14 px-3 rounded bg-control">
                    <span class="text-reading flex-1 text-shell-ink">Discard unsaved changes?</span>
                    <button
                      type="button"
                      class="h-11 px-3 rounded text-xs text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
                      onClick={() => setAsking(null)}
                    >
                      Keep
                    </button>
                    <button
                      type="button"
                      class="h-11 px-3 rounded text-xs font-medium text-on-primary bg-primary hover:bg-primary-hover transition-colors focus-ring"
                      onClick={() => {
                        setAsking(null);
                        props.onClose(tab.id);
                      }}
                    >
                      Discard
                    </button>
                  </div>
                </Show>
                {/* The tint belongs to the ROW, not to the part of it that
                    picks the tab: with it on the button alone, the fill
                    stopped short of the close control and the selected row
                    read as a clipped band. */}
                <div
                  class={`flex items-stretch overflow-hidden rounded transition-colors ${
                    tab.id === props.activeId ? 'bg-primary/10' : 'hover:bg-hover-wash'
                  }`}
                >
                <button
                  type="button"
                  class={`flex-1 min-w-0 flex items-center gap-2 h-14 px-3 rounded text-left focus-ring ${
                    tab.id === props.activeId ? 'text-shell-ink' : 'text-shell-body'
                  }`}
                  onClick={() => props.onPick(tab.id)}
                >
                  <Dynamic
                    component={iconForContentType(tab.contentType) ?? X}
                    class="w-4 h-4 shrink-0 text-muted-dark"
                  />
                  <span class="text-reading flex-1 truncate">{tab.title}</span>
                  <Show when={tab.isModified}>
                    <span
                      aria-label={`${tab.title} has unsaved changes`}
                      class="w-2 h-2 rounded-full bg-primary shrink-0"
                    />
                  </Show>
                </button>
                <button
                  type="button"
                  aria-label={`Close ${tab.title}`}
                  class="w-11 shrink-0 flex items-center justify-center rounded text-muted-dark hover:text-shell-ink focus-ring"
                  onClick={() => requestClose(tab)}
                >
                  <X class="w-4 h-4" />
                </button>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};
