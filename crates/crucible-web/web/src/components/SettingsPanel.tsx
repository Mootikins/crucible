// src/components/SettingsPanel.tsx
import { Component, Show, For, ErrorBoundary, createSignal, onMount, onCleanup } from 'solid-js';
import { useSettings } from '@/contexts/SettingsContext';
import { useSessionSafe } from '@/contexts/SessionContext';
import type { TranscriptionProvider } from '@/lib/settings';
import type { PluginInfo } from '@/lib/api';
import {
  getThinkingBudget,
  setThinkingBudget as apiSetThinkingBudget,
  getTemperature,
  setTemperature as apiSetTemperature,
  getMaxTokens,
  setMaxTokens as apiSetMaxTokens,
  getPrecognition,
  setPrecognition as apiSetPrecognition,
  getPlugins,
  reloadPlugin,
  getMcpStatus,
} from '@/lib/api';

// =============================================================================
// Section Header
// =============================================================================

const SectionHeader: Component<{ title: string; icon: string }> = (props) => (
  <tr>
    <td
      colSpan={2}
      class="pt-6 pb-2 text-xs font-semibold uppercase tracking-wider text-neutral-400 border-b border-neutral-700"
    >
      <span class="mr-1.5">{props.icon}</span>
      {props.title}
    </td>
  </tr>
);

// =============================================================================
// Debounce helper
// =============================================================================

function createDebounce<T extends (...args: unknown[]) => void>(fn: T, delay: number) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const debounced = (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delay);
  };
  const cleanup = () => {
    if (timer) clearTimeout(timer);
  };
  return { debounced, cleanup };
}

// =============================================================================
// Model Settings Section
// =============================================================================

const ModelSettingsSection: Component = () => {
  const session = useSessionSafe();

  const [thinkingBudget, setThinkingBudget] = createSignal<number | null>(null);
  const [temperature, setTemperature] = createSignal<number>(1.0);
  const [, setMaxTokens] = createSignal<number | null>(null);
  const [maxTokensText, setMaxTokensText] = createSignal('');
  const [precognition, setPrecognition] = createSignal(true);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  // Debounced API callers
  const budgetDebounce = createDebounce(async (...args: unknown[]) => {
    const [sid, val] = args as [string, number | null];
    try {
      await apiSetThinkingBudget(sid, val);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set thinking budget');
    }
  }, 300);

  const tempDebounce = createDebounce(async (...args: unknown[]) => {
    const [sid, val] = args as [string, number];
    try {
      await apiSetTemperature(sid, val);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set temperature');
    }
  }, 300);

  onCleanup(() => {
    budgetDebounce.cleanup();
    tempDebounce.cleanup();
  });

  const loadSettings = async () => {
    const s = session.currentSession();
    if (!s) {
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const [budget, temp, tokens, precog] = await Promise.all([
        getThinkingBudget(s.id),
        getTemperature(s.id),
        getMaxTokens(s.id),
        getPrecognition(s.id),
      ]);
      setThinkingBudget(budget);
      setTemperature(temp ?? 1.0);
      setMaxTokens(tokens);
      setMaxTokensText(tokens !== null ? String(tokens) : '');
      setPrecognition(precog);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load settings');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadSettings);

  const inputClass = 'bg-neutral-800 border border-neutral-600 rounded px-2 py-1 text-sm text-white focus:border-blue-500 focus:outline-none';

  const handleBudgetChange = (e: Event) => {
    const val = parseInt((e.target as HTMLInputElement).value, 10);
    const budget = isNaN(val) ? null : Math.max(0, Math.min(32768, val));
    setThinkingBudget(budget);
    const s = session.currentSession();
    if (s) budgetDebounce.debounced(s.id, budget);
  };

  const handleTemperatureChange = (e: Event) => {
    const val = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(val)) {
      setTemperature(val);
      const s = session.currentSession();
      if (s) tempDebounce.debounced(s.id, val);
    }
  };

  const handleMaxTokensChange = async (e: Event) => {
    const raw = (e.target as HTMLInputElement).value.trim();
    setMaxTokensText(raw);
    const s = session.currentSession();
    if (!s) return;

    const val = raw === '' ? null : parseInt(raw, 10);
    if (raw !== '' && isNaN(val as number)) return;

    setMaxTokens(val);
    try {
      await apiSetMaxTokens(s.id, val);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set max tokens');
    }
  };

  const handlePrecognitionToggle = async () => {
    const s = session.currentSession();
    if (!s) return;

    const newVal = !precognition();
    setPrecognition(newVal);
    try {
      await apiSetPrecognition(s.id, newVal);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set precognition');
      setPrecognition(!newVal); // revert
    }
  };

  const labelClass = 'text-neutral-300 text-sm';

  return (
    <>
      <SectionHeader title="Model Settings" icon="🧠" />

      <Show when={!session.currentSession()}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
            No active session — start a chat to configure model settings.
          </td>
        </tr>
      </Show>

      <Show when={session.currentSession()}>
        <Show when={loading()}>
          <tr>
            <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
              Loading settings…
            </td>
          </tr>
        </Show>

        <Show when={error()}>
          <tr>
            <td colSpan={2} class="py-2 text-center text-red-400 text-xs">
              {error()}
            </td>
          </tr>
        </Show>

        <Show when={!loading()}>
          {/* Thinking Budget */}
          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>
              <div>Thinking Budget</div>
              <div class="text-xs text-neutral-500">0–32768 tokens</div>
            </td>
            <td class="py-3 text-right">
              <input
                type="number"
                min={0}
                max={32768}
                step={1024}
                value={thinkingBudget() ?? ''}
                onInput={handleBudgetChange}
                class={`${inputClass} w-28 text-right`}
                placeholder="Auto"
              />
            </td>
          </tr>

          {/* Temperature */}
          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>
              <div>Temperature</div>
              <div class="text-xs text-neutral-500">{temperature().toFixed(1)}</div>
            </td>
            <td class="py-3 text-right flex items-center justify-end gap-2">
              <span class="text-xs text-neutral-500">0</span>
              <input
                type="range"
                min={0}
                max={2}
                step={0.1}
                value={temperature()}
                onInput={handleTemperatureChange}
                class="w-32 accent-blue-500"
              />
              <span class="text-xs text-neutral-500">2</span>
            </td>
          </tr>

          {/* Max Tokens */}
          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>
              <div>Max Tokens</div>
              <div class="text-xs text-neutral-500">Empty = unlimited</div>
            </td>
            <td class="py-3 text-right">
              <input
                type="number"
                min={1}
                value={maxTokensText()}
                onBlur={handleMaxTokensChange}
                onInput={(e) => setMaxTokensText((e.target as HTMLInputElement).value)}
                class={`${inputClass} w-28 text-right`}
                placeholder="Unlimited"
              />
            </td>
          </tr>

          {/* Precognition */}
          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>
              <div>Precognition</div>
              <div class="text-xs text-neutral-500">Auto-inject context</div>
            </td>
            <td class="py-3 text-right">
              <button
                onClick={handlePrecognitionToggle}
                class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                  precognition() ? 'bg-blue-600' : 'bg-neutral-600'
                }`}
              >
                <span
                  class={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                    precognition() ? 'translate-x-6' : 'translate-x-1'
                  }`}
                />
              </button>
            </td>
          </tr>
        </Show>
      </Show>
    </>
  );
};

// =============================================================================
// Plugins Section
// =============================================================================

const PluginsSection: Component = () => {
  const session = useSessionSafe();

  const [plugins, setPlugins] = createSignal<PluginInfo[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [reloadingPlugin, setReloadingPlugin] = createSignal<string | null>(null);

  const loadPlugins = async () => {
    const s = session.currentSession();
    if (!s?.kiln) {
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const list = await getPlugins(s.kiln);
      setPlugins(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load plugins');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadPlugins);

  const handleReload = async (name: string) => {
    setReloadingPlugin(name);
    try {
      await reloadPlugin(name);
      // Refresh list after reload
      await loadPlugins();
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to reload ${name}`);
    } finally {
      setReloadingPlugin(null);
    }
  };

  const labelClass = 'text-neutral-300 text-sm';

  return (
    <>
      <SectionHeader title="Plugins" icon="🔌" />

      <Show when={!session.currentSession()}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
            No active session — start a chat to view plugins.
          </td>
        </tr>
      </Show>

      <Show when={session.currentSession()}>
        <Show when={loading()}>
          <tr>
            <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
              Loading plugins…
            </td>
          </tr>
        </Show>

        <Show when={error()}>
          <tr>
            <td colSpan={2} class="py-2 text-center text-red-400 text-xs">
              {error()}
            </td>
          </tr>
        </Show>

        <Show when={!loading() && plugins().length === 0}>
          <tr>
            <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
              No plugins discovered.
            </td>
          </tr>
        </Show>

        <Show when={!loading() && plugins().length > 0}>
          <For each={plugins()}>
            {(plugin) => (
              <tr class="border-b border-neutral-700">
                <td class={`py-2.5 ${labelClass}`}>
                  <div class="flex items-center gap-2">
                    <span
                      class={`inline-block w-2 h-2 rounded-full ${
                        plugin.healthy === false ? 'bg-red-500' : 'bg-emerald-500'
                      }`}
                    />
                    <div>
                      <div class="text-sm">{plugin.name}</div>
                      <div class="text-xs text-neutral-500">{plugin.plugin_type}</div>
                    </div>
                  </div>
                </td>
                <td class="py-2.5 text-right">
                  <button
                    onClick={() => handleReload(plugin.name)}
                    disabled={reloadingPlugin() === plugin.name}
                    class="px-2 py-1 text-xs rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {reloadingPlugin() === plugin.name ? '↻' : 'Reload'}
                  </button>
                </td>
              </tr>
            )}
          </For>
        </Show>
      </Show>
    </>
  );
};

// =============================================================================
// MCP Status Section
// =============================================================================

const McpStatusSection: Component = () => {
  const [status, setStatus] = createSignal<Record<string, unknown> | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  const loadStatus = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getMcpStatus();
      setStatus(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load MCP status');
    } finally {
      setLoading(false);
    }
  };

  onMount(loadStatus);

  const labelClass = 'text-neutral-300 text-sm';

  return (
    <>
      <SectionHeader title="MCP Status" icon="🔗" />

      <Show when={loading()}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
            Loading MCP status…
          </td>
        </tr>
      </Show>

      <Show when={error()}>
        <tr>
          <td colSpan={2} class="py-2">
            <div class="text-center text-red-400 text-xs">{error()}</div>
            <div class="text-center mt-1">
              <button
                onClick={loadStatus}
                class="px-2 py-1 text-xs rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors"
              >
                Retry
              </button>
            </div>
          </td>
        </tr>
      </Show>

      <Show when={!loading() && !error() && status()}>
        <For each={Object.entries(status()!)}>
          {([key, value]) => (
            <tr class="border-b border-neutral-700">
              <td class={`py-2.5 ${labelClass}`}>{key}</td>
              <td class="py-2.5 text-right text-sm text-neutral-400 max-w-[200px] truncate">
                {typeof value === 'object' ? JSON.stringify(value) : String(value ?? '—')}
              </td>
            </tr>
          )}
        </For>
      </Show>

      <Show when={!loading() && !error() && !status()}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-neutral-500 text-sm">
            No MCP status available.
          </td>
        </tr>
      </Show>
    </>
  );
};

// =============================================================================
// Main Settings Panel
// =============================================================================

const SettingsPanelContent: Component = () => {
  const { settings, updateSetting } = useSettings();

  const handleProviderChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value as TranscriptionProvider;
    updateSetting('transcription', 'provider', value);
  };

  const handleUrlChange = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    updateSetting('transcription', 'serverUrl', value);
  };

  const handleModelChange = (e: Event) => {
    const value = (e.target as HTMLInputElement).value;
    updateSetting('transcription', 'model', value);
  };

  const handleLanguageChange = (e: Event) => {
    const value = (e.target as HTMLSelectElement).value;
    updateSetting('transcription', 'language', value);
  };

  const inputClass = 'bg-neutral-800 border border-neutral-600 rounded px-2 py-1 text-sm text-white focus:border-blue-500 focus:outline-none';
  const selectClass = `${inputClass} cursor-pointer`;
  const labelClass = 'text-neutral-300 text-sm';

  return (
    <div class="h-full bg-neutral-900 p-4 overflow-auto">
      <table class="w-full">
        <tbody>
          {/* Transcription Settings */}
          <SectionHeader title="Transcription" icon="🎙️" />

          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>Provider</td>
            <td class="py-3 text-right">
              <select
                value={settings.transcription.provider}
                onChange={handleProviderChange}
                class={selectClass}
              >
                <option value="local">Local (WebGPU)</option>
                <option value="server">Server</option>
              </select>
            </td>
          </tr>

          <Show when={settings.transcription.provider === 'server'}>
            <tr class="border-b border-neutral-700">
              <td class={`py-3 ${labelClass}`}>Whisper URL</td>
              <td class="py-3 text-right">
                <input
                  type="text"
                  value={settings.transcription.serverUrl}
                  onInput={handleUrlChange}
                  class={`${inputClass} w-64`}
                  placeholder="https://whisper.example.com"
                />
              </td>
            </tr>

            <tr class="border-b border-neutral-700">
              <td class={`py-3 ${labelClass}`}>Whisper Model</td>
              <td class="py-3 text-right">
                <input
                  type="text"
                  value={settings.transcription.model}
                  onInput={handleModelChange}
                  class={`${inputClass} w-48`}
                  placeholder="whisper-large-v3-turbo"
                />
              </td>
            </tr>

            <tr class="border-b border-neutral-700">
              <td class={`py-3 ${labelClass}`}>Language</td>
              <td class="py-3 text-right">
                <select
                  value={settings.transcription.language}
                  onChange={handleLanguageChange}
                  class={selectClass}
                >
                  <option value="auto">Auto-detect</option>
                  <option value="en">English</option>
                  <option value="es">Spanish</option>
                  <option value="fr">French</option>
                  <option value="de">German</option>
                  <option value="zh">Chinese</option>
                  <option value="ja">Japanese</option>
                </select>
              </td>
            </tr>
          </Show>

          {/* Model Settings Section */}
          <ModelSettingsSection />

          {/* Plugins Section */}
          <PluginsSection />

          {/* MCP Status Section */}
          <McpStatusSection />
        </tbody>
      </table>
    </div>
  );
};

/**
 * Wrapper component that safely renders SettingsPanel with error handling.
 * Catches context errors and displays a fallback message.
 */
export const SettingsPanel: Component = () => {
  return (
    <ErrorBoundary fallback={(err) => (
      <div class="h-full bg-neutral-900 p-4 flex items-center justify-center">
        <div class="text-center text-neutral-400">
          <div class="text-sm mb-2">⚠️ Settings Error</div>
          <div class="text-xs text-neutral-500">{String(err)}</div>
        </div>
      </div>
    )}>
      <SettingsPanelContent />
    </ErrorBoundary>
  );
};
