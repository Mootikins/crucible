import { Component, For } from 'solid-js';
import { shortcutLabel } from '@/lib/keyboard-shortcuts';

/**
 * What an empty centre pane shows.
 *
 * An empty pane used to draw nothing at all, which said the right thing —
 * starting a session is a deliberate act — and read as a rendering failure:
 * a third of a wide viewport went black with no content, no placeholder and
 * no hint. This is the smallest correction that keeps the intent. It names the
 * state, it names the two keys that fill the pane, and it stops there. No
 * illustration, no splash, no copy that asks to be read.
 *
 * The chords come from `DEFAULT_SHORTCUTS`, so a rebound key reprints here and
 * the hint can never name a key the app does not listen for.
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
  const hints = () =>
    [
      { action: 'openNoteSwitcher', label: 'Open a note' },
      { action: 'openCommandPalette', label: 'Command palette' },
    ]
      .map((hint) => ({ ...hint, chord: shortcutLabel(hint.action) }))
      .filter((hint): hint is typeof hint & { chord: string } => hint.chord !== null);

  return (
    <div
      class="flex-1 flex items-center justify-center overflow-hidden p-3"
      data-testid="empty-pane"
      data-empty-pane={props.solitary ? 'region' : 'pane'}
    >
      <div class="w-full max-w-[15rem] rounded-md border border-hairline px-3 py-2.5 text-muted">
        <p class="text-floor leading-4">{props.solitary ? 'Nothing open' : 'Empty pane'}</p>
        <ul class="mt-2 flex flex-col gap-1">
          <For each={hints()}>
            {(hint) => (
              <li class="flex items-center justify-between gap-2 text-floor leading-4">
                <span class="truncate">{hint.label}</span>
                <kbd class="flex-none rounded border border-hairline bg-surface-overlay px-1.5 py-0.5 text-floor text-muted">
                  {hint.chord}
                </kbd>
              </li>
            )}
          </For>
        </ul>
      </div>
    </div>
  );
};
