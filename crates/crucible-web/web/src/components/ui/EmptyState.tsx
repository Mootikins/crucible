import { Component, For, Show } from 'solid-js';

/** One offer on an empty state. `kbd` names the shortcut that does the same. */
export interface EmptyStateAction {
  label: string;
  onClick: () => void;
  kbd?: string;
}

/**
 * The one empty state.
 *
 * Every panel wrote its own before this: an 8px icon on its own row over two
 * grey lines in one panel, a bare centred sentence in the next, and no offer
 * of a way out in either. There is no icon: the panel header carries one,
 * and every empty pane reads the same way.
 *
 * `tone` separates two different facts. `empty` says the panel asked and the
 * answer was nothing. `error` says the panel could not ask. A failure that
 * renders as an empty state tells the user a lie about their data, so the
 * error tone carries the error colour on the icon, a hairline in the error
 * colour, and normally a Retry action.
 */
export const EmptyState: Component<{
  title: string;
  body?: string;
  /** One offer, or two. Rendered as small control buttons under the body. */
  action?: EmptyStateAction | EmptyStateAction[];
  tone?: 'empty' | 'error';
  /** Half the vertical padding, for a state INSIDE a panel section rather
   * than one that fills the panel. Not a `class` override: Tailwind orders
   * `py-4` before `py-8` in its output, so an appended class loses. */
  compact?: boolean;
  class?: string;
  testid?: string;
}> = (props) => {
  const tone = () => props.tone ?? 'empty';
  const actions = (): EmptyStateAction[] => {
    const a = props.action;
    if (!a) return [];
    return Array.isArray(a) ? a : [a];
  };

  return (
    <div
      class={`empty-state flex flex-col items-center justify-center gap-1.5 px-6 text-center ${
        props.compact ? 'py-4' : 'py-8'
      } ${props.class ?? ''}`}
      data-tone={tone()}
      data-testid={props.testid ?? 'empty-state'}
      role={tone() === 'error' ? 'alert' : undefined}
    >
      {/* No icon: the panel header already carries one, and every empty
          pane reads the same way. */}
      <p class="empty-state-title text-title font-semibold text-shell-ink">{props.title}</p>
      <Show when={props.body}>
        <p class="empty-state-body text-reading text-muted-dark max-w-(--cru-measure-empty)">{props.body}</p>
      </Show>
      <Show when={actions().length > 0}>
        <div class="mt-1.5 flex items-center justify-center gap-1.5">
          <For each={actions()}>
            {(action) => (
              <button
                type="button"
                data-testid="empty-state-action"
                class="focus-ring inline-flex items-center gap-1.5 rounded bg-control px-2.5 py-1 text-reading text-shell-ink transition-colors hover:bg-hover-wash"
                onClick={() => action.onClick()}
              >
                <span>{action.label}</span>
                <Show when={action.kbd}>
                  <kbd class="text-floor font-mono text-muted-dark">{action.kbd}</kbd>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};
