import { Component, For, JSX, createSignal } from 'solid-js';

export interface DrawerTab {
  id: string;
  label: string;
  content: () => JSX.Element;
}

/**
 * A drawer's tab strip. The tabs never move — decision log 2026-08-13, "edge
 * drawers with tabs but no tab MOVEMENT", because movement is where the window
 * manager's bulk lives.
 *
 * Every panel stays mounted and only one is shown, so a file tree keeps its
 * expansion and a session list its scroll when the user flips between them.
 */
export const DrawerTabs: Component<{ tabs: DrawerTab[]; label: string }> = (props) => {
  const [active, setActive] = createSignal(props.tabs[0]?.id);
  const tabId = (id: string) => `drawer-tab-${id}`;
  const panelId = (id: string) => `drawer-panel-${id}`;

  return (
    <div class="flex-1 min-h-0 flex flex-col">
      <div role="tablist" aria-label={props.label} class="shrink-0 flex border-b border-hairline bg-surface-elevated">
        <For each={props.tabs}>
          {(tab) => (
            <button
              type="button"
              role="tab"
              id={tabId(tab.id)}
              aria-selected={active() === tab.id}
              aria-controls={panelId(tab.id)}
              // The tab strip reads like the desktop's: an active tab in
              // shell ink over the panel surface, the rest muted, the active
              // one marked by the primary rule the window manager uses.
              class={`flex-1 h-11 text-xs font-medium transition-colors focus-ring ${
                active() === tab.id
                  ? 'text-shell-ink bg-surface-base border-b-2 border-primary'
                  : 'text-muted-dark hover:text-shell-ink hover:bg-hover-wash'
              }`}
              onClick={() => setActive(tab.id)}
            >
              {tab.label}
            </button>
          )}
        </For>
      </div>
      <For each={props.tabs}>
        {(tab) => (
          <div
            role="tabpanel"
            id={panelId(tab.id)}
            aria-labelledby={tabId(tab.id)}
            hidden={active() !== tab.id}
            class="flex-1 min-h-0 flex flex-col overflow-hidden"
          >
            {tab.content()}
          </div>
        )}
      </For>
    </div>
  );
};
