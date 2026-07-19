import { Component, Show, For, createSignal, createResource, createMemo, onCleanup } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { listSkills, searchSkills, getSkill, getConfig } from '@/lib/api';
import type { SkillSummary, SkillDetail } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';

const SEARCH_DEBOUNCE_MS = 200;

/**
 * Resolves the kiln path to use for skills queries. Prefers the active
 * session's kiln; falls back to the daemon's configured default.
 */
function useKilnPath() {
  const { currentSession } = useSessionSafe();
  return createResource<string | null>(async () => {
    const sess = currentSession();
    if (sess?.kiln) return sess.kiln;
    try {
      const config = await getConfig();
      return config.kiln_path || null;
    } catch {
      return null;
    }
  });
}

export const SkillsPanel: Component = () => {
  const [kilnPath] = useKilnPath();
  const [query, setQuery] = createSignal('');
  const [debouncedQuery, setDebouncedQuery] = createSignal('');
  const [selected, setSelected] = createSignal<SkillSummary | null>(null);
  const [detail, setDetail] = createSignal<SkillDetail | null>(null);
  const [detailLoading, setDetailLoading] = createSignal(false);

  // Debounce typed query so server-side search isn't fired on every keystroke.
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  const onQueryInput = (value: string) => {
    setQuery(value);
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => setDebouncedQuery(value), SEARCH_DEBOUNCE_MS);
  };
  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  const [skills] = createResource(
    () => ({ kiln: kilnPath(), q: debouncedQuery() }),
    async ({ kiln, q }) => {
      if (!kiln) return [];
      try {
        return q.trim().length > 0
          ? await searchSkills(q, kiln)
          : await listSkills(kiln);
      } catch (err) {
        notificationActions.addNotification('error', `Failed to load skills: ${err}`);
        return [];
      }
    },
  );

  const groupedSkills = createMemo(() => {
    const list = skills() ?? [];
    const groups = new Map<string, SkillSummary[]>();
    for (const skill of list) {
      const bucket = groups.get(skill.scope) ?? [];
      bucket.push(skill);
      groups.set(skill.scope, bucket);
    }
    // Stable, alphabetical scope order.
    return Array.from(groups.entries()).sort(([a], [b]) => a.localeCompare(b));
  });

  const openDetail = async (skill: SkillSummary) => {
    setSelected(skill);
    setDetail(null);
    const kiln = kilnPath();
    if (!kiln) return;
    setDetailLoading(true);
    try {
      const full = await getSkill(skill.name, kiln);
      setDetail(full);
    } catch (err) {
      notificationActions.addNotification('error', `Failed to load skill: ${err}`);
    } finally {
      setDetailLoading(false);
    }
  };

  const closeDetail = () => {
    setSelected(null);
    setDetail(null);
  };

  const copyInvocation = async (name: string) => {
    try {
      await navigator.clipboard.writeText(`/${name}`);
      notificationActions.addNotification('success', `Copied /${name} to clipboard`);
    } catch {
      notificationActions.addNotification('error', 'Clipboard copy failed');
    }
  };

  return (
    <PanelShell class="relative">
      <PanelHeader title="Skills" />
      <div class="px-3 py-2 border-b border-hairline">
        <input
          type="search"
          value={query()}
          onInput={(e) => onQueryInput(e.currentTarget.value)}
          placeholder="Search skills…"
          class="w-full bg-control text-shell-ink text-sm rounded px-2 py-1.5 placeholder-muted-dark border border-hairline focus:outline-none focus:border-muted-dark"
          data-testid="skills-search-input"
        />
      </div>

      <div class="flex-1 overflow-y-auto">
        <Show
          when={kilnPath()}
          fallback={<div class="p-4 text-sm text-muted-dark">No kiln selected.</div>}
        >
          <Show
            when={!skills.loading}
            fallback={<div class="p-4 text-sm text-muted-dark">Loading…</div>}
          >
            <Show
              when={(skills() ?? []).length > 0}
              fallback={
                <div class="p-4 text-sm text-muted-dark">
                  {query() ? 'No matching skills.' : 'No skills discovered.'}
                </div>
              }
            >
              <For each={groupedSkills()}>
                {([scope, items]) => (
                  <div class="mb-2">
                    <div class="px-3 py-1 text-[10px] uppercase tracking-wider text-muted-dark bg-surface-elevated">
                      {scope}
                    </div>
                    <For each={items}>
                      {(skill) => (
                        <button
                          type="button"
                          class="w-full text-left px-3 py-2 hover:bg-hover-wash border-b border-hairline focus:outline-none focus:bg-hover-wash"
                          onClick={() => openDetail(skill)}
                          data-testid={`skill-row-${skill.name}`}
                        >
                          <div class="flex items-center gap-2">
                            <span class="text-sm font-mono text-shell-ink truncate flex-1">
                              {skill.name}
                            </span>
                            <Show when={skill.shadowed_count > 0}>
                              <span
                                class="text-[10px] px-1.5 py-0.5 rounded bg-attention/15 text-attention border border-attention/50"
                                title={`Shadows ${skill.shadowed_count} other skill(s) with the same name`}
                              >
                                +{skill.shadowed_count}
                              </span>
                            </Show>
                          </div>
                          <div class="text-xs text-muted-dark truncate mt-0.5">
                            {skill.description || 'No description'}
                          </div>
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </Show>
      </div>

      <Show when={selected()}>
        {(s) => (
          <div
            class="absolute inset-0 bg-surface-overlay z-10 flex flex-col"
            data-testid="skills-drawer"
          >
            <div class="flex items-center gap-2 px-3 py-2 border-b border-hairline">
              <button
                type="button"
                onClick={closeDetail}
                class="text-muted hover:text-shell-ink text-sm"
                data-testid="skills-drawer-close"
              >
                ← Back
              </button>
              <span class="flex-1 text-sm font-mono text-shell-ink truncate">{s().name}</span>
              <span class="text-[10px] uppercase tracking-wider text-muted-dark">
                {s().scope}
              </span>
            </div>
            <div class="flex-1 overflow-y-auto p-3">
              <button
                type="button"
                onClick={() => copyInvocation(s().name)}
                class="mb-3 text-xs px-2 py-1 bg-control hover:bg-hover-wash rounded border border-hairline text-shell-ink"
                data-testid="skills-copy-invocation"
              >
                Copy /{s().name}
              </button>
              <Show when={!detailLoading()} fallback={<div class="text-sm text-muted-dark">Loading…</div>}>
                <Show when={detail()}>
                  {(d) => (
                    <>
                      <Show when={d().description}>
                        <p class="text-sm text-shell-body mb-3">{d().description}</p>
                      </Show>
                      <pre class="text-xs font-mono text-shell-body whitespace-pre-wrap break-words bg-shell-bg p-3 rounded border border-hairline">
                        {d().body}
                      </pre>
                    </>
                  )}
                </Show>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </PanelShell>
  );
};
