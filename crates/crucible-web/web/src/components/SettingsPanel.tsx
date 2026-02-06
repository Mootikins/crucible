// src/components/SettingsPanel.tsx
import { Component, Show, ErrorBoundary } from 'solid-js';
import { useSettings } from '@/contexts/SettingsContext';
import type { TranscriptionProvider } from '@/lib/settings';

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
          <tr class="border-b border-neutral-700">
            <td class={`py-3 ${labelClass}`}>Transcription</td>
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
