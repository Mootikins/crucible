import { Component, For, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useChatSafe } from '@/contexts/ChatContext';
import {
  connectSessionKiln,
  disconnectSessionKiln,
  listKilns,
  listProjects,
  setSessionWorkspace,
} from '@/lib/api';
import type { KilnListEntry, Project } from '@/lib/types';
import { notificationActions } from '@/stores/notificationStore';
import { pathBasename } from '@/stores/statusBarStore';

/**
 * Interactive session-scope strip above the chat input: the workspace the
 * session acts in and the kilns it knows. Attach/detach mid-session — the
 * daemon rejects mutations mid-turn, re-checks trust on attach, and rebuilds
 * the agent's tools/prompt on the next turn.
 */
export const SessionScopeChips: Component = () => {
  const { currentSession, applySessionScope } = useSessionSafe();
  const { isStreaming } = useChatSafe();

  const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
  const [projects, setProjects] = createSignal<Project[]>([]);
  const [openPicker, setOpenPicker] = createSignal<'kiln' | 'project' | null>(null);
  const [busy, setBusy] = createSignal(false);

  const session = () => currentSession();
  // Workspace == kiln is the daemon's "no workspace" state (Session::new).
  const hasWorkspace = () => {
    const s = session();
    return !!s && s.workspace !== s.kiln;
  };
  const disabled = () => busy() || isStreaming();

  onMount(() => {
    void listKilns().then(setKilns).catch(() => {});
    void listProjects().then(setProjects).catch(() => {});
  });

  // Dismiss the open kiln/project picker on Escape or any outside click.
  // The listeners are attached on the next tick so the opening click (which
  // the toggle button stops from propagating) doesn't immediately close it.
  createEffect(() => {
    if (!openPicker()) return;
    const close = () => setOpenPicker(null);
    const onEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpenPicker(null);
    };
    const timer = setTimeout(() => {
      document.addEventListener('click', close);
      document.addEventListener('keydown', onEscape);
    }, 0);
    onCleanup(() => {
      clearTimeout(timer);
      document.removeEventListener('click', close);
      document.removeEventListener('keydown', onEscape);
    });
  });

  const mutate = async (action: () => Promise<Parameters<typeof applySessionScope>[0]>) => {
    if (disabled()) return;
    setBusy(true);
    setOpenPicker(null);
    try {
      applySessionScope(await action());
    } catch (err) {
      notificationActions.addNotification(
        'error',
        err instanceof Error ? err.message : 'Failed to update session scope',
      );
    } finally {
      setBusy(false);
    }
  };

  const attachableKilns = () => {
    const s = session();
    if (!s) return [];
    return kilns().filter((k) => k.path !== s.kiln && !s.connected_kilns.includes(k.path));
  };

  const chipButton =
    'ml-1 text-muted-dark hover:text-error transition-colors disabled:opacity-50';
  const addButton =
    'text-[11px] text-muted-dark hover:text-shell-ink border border-dashed border-hairline rounded-full px-2 py-0.5 transition-colors disabled:opacity-50';
  const pickerClass =
    'absolute bottom-full left-0 mb-1 min-w-[200px] max-w-[320px] max-h-56 overflow-y-auto bg-surface-elevated border border-hairline rounded-lg shadow-xl z-50';
  const pickerItem =
    'w-full px-3 py-1.5 text-left text-xs text-shell-ink hover:bg-surface-overlay transition-colors truncate';

  return (
    <Show when={session()}>
      <div class="flex items-center gap-1.5 flex-wrap mb-2" data-testid="context-chips">
        {/* Workspace: attach a project or detach back to floating. */}
        <Show
          when={hasWorkspace()}
          fallback={
            <div class="relative">
              <button
                type="button"
                class={addButton}
                disabled={disabled()}
                aria-haspopup="menu"
                aria-expanded={openPicker() === 'project'}
                onClick={(e) => {
                  e.stopPropagation();
                  setOpenPicker(openPicker() === 'project' ? null : 'project');
                }}
                data-testid="attach-project"
              >
                ⌁ + project
              </button>
              <Show when={openPicker() === 'project'}>
                <div class={pickerClass}>
                  <Show
                    when={projects().length > 0}
                    fallback={<div class="px-3 py-2 text-xs text-muted-dark">No registered projects</div>}
                  >
                    <For each={projects()}>
                      {(p) => (
                        <button
                          type="button"
                          class={pickerItem}
                          onClick={() => void mutate(() =>
                            setSessionWorkspace(session()!.id, p.path),
                          )}
                        >
                          {p.name || pathBasename(p.path)} — {p.path}
                        </button>
                      )}
                    </For>
                  </Show>
                </div>
              </Show>
            </div>
          }
        >
          <span
            title={`workspace · ${session()!.workspace}`}
            class="font-mono text-[11px] text-attention bg-attention/10 border border-attention/40 rounded-full px-2.5 py-0.5 inline-flex items-center"
            data-testid="workspace-chip"
          >
            ⌁ {pathBasename(session()!.workspace)}
            <button
              type="button"
              class={chipButton}
              title="Detach workspace"
              disabled={disabled()}
              onClick={() => void mutate(() => setSessionWorkspace(session()!.id, null))}
              data-testid="detach-workspace"
            >
              ✕
            </button>
          </span>
        </Show>

        <span class="w-px h-4 bg-hairline" />

        {/* Primary kiln: fixed — the session is stored there. */}
        <span
          title={`kiln · ${session()!.kiln}`}
          class="text-[11.5px] text-shell-ink bg-primary/10 border border-primary/45 rounded-full px-2.5 py-0.5"
          data-testid="primary-kiln-chip"
        >
          ◆ {pathBasename(session()!.kiln)}
        </span>

        {/* Connected kilns: detachable. */}
        <For each={session()!.connected_kilns}>
          {(kiln) => (
            <span
              title={`connected kiln · ${kiln}`}
              class="text-[11.5px] text-shell-body bg-primary/5 border border-hairline rounded-full px-2.5 py-0.5 inline-flex items-center"
              data-testid="connected-kiln-chip"
            >
              ◇ {pathBasename(kiln)}
              <button
                type="button"
                class={chipButton}
                title="Detach kiln"
                disabled={disabled()}
                onClick={() => void mutate(() => disconnectSessionKiln(session()!.id, kiln))}
                data-testid={`detach-kiln-${pathBasename(kiln)}`}
              >
                ✕
              </button>
            </span>
          )}
        </For>

        <div class="relative">
          <button
            type="button"
            class={addButton}
            disabled={disabled()}
            aria-haspopup="menu"
            aria-expanded={openPicker() === 'kiln'}
            onClick={(e) => {
              e.stopPropagation();
              setOpenPicker(openPicker() === 'kiln' ? null : 'kiln');
            }}
            data-testid="attach-kiln"
          >
            ◇ + kiln
          </button>
          <Show when={openPicker() === 'kiln'}>
            <div class={pickerClass}>
              <Show
                when={attachableKilns().length > 0}
                fallback={<div class="px-3 py-2 text-xs text-muted-dark">No other kilns available</div>}
              >
                <For each={attachableKilns()}>
                  {(k) => (
                    <button
                      type="button"
                      class={pickerItem}
                      onClick={() => void mutate(() => connectSessionKiln(session()!.id, k.path))}
                    >
                      {k.name || pathBasename(k.path)} — {k.path}
                    </button>
                  )}
                </For>
              </Show>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};
