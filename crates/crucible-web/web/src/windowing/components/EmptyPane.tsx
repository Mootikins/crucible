import { Component, For, Show } from 'solid-js';
import { useWindowing } from '@/windowing/components/context';

/**
 * What an empty centre pane shows.
 *
 * An empty pane used to draw nothing at all, which said the right thing —
 * filling a pane is a deliberate act — and read as a rendering failure:
 * a third of a wide viewport went black with no content, no placeholder and
 * no hint. This is the smallest correction that keeps the intent. It names the
 * state, it names the two keys that fill the pane, and it stops there. No
 * illustration, no splash, no copy that asks to be read.
 *
 * The rows come from the `emptyPaneHints` slot. The app knows its keys; the
 * window manager does not.
 */
export const EmptyPane: Component<{
  /**
   * No other pane in this region holds a tab.
   *
   * The two empty states read differently: a region with nothing open is the
   * app at rest, while one empty pane beside panes that hold work is a pane
   * the user emptied on purpose.
   */
  solitary: boolean;
}> = (props) => {
  const windowing = useWindowing();
  const hints = () => windowing.slots.emptyPaneHints?.() ?? [];

  return (
    <div
      class="wm-empty-pane flex-1 flex items-center justify-center overflow-hidden"
      data-testid="empty-pane"
      data-empty-pane={props.solitary ? 'region' : 'pane'}
    >
      <div class="wm-empty-card w-full max-w-[15rem]">
        <p class="wm-empty-title">{props.solitary ? 'Nothing open' : 'Empty pane'}</p>
        <Show when={hints().length > 0}>
          <ul class="wm-empty-hints flex flex-col">
            <For each={hints()}>
              {(hint) => (
                <li class="wm-empty-hint flex items-center justify-between">
                  <span class="truncate">{hint.label}</span>
                  <kbd class="wm-empty-kbd flex-none">
                    {hint.chord}
                  </kbd>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
    </div>
  );
};
