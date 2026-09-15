// src/components/settings/TranscriptionSettings.tsx
import { Component, Show } from 'solid-js';
import { Mic } from '@/lib/icons';

import { SectionHeader } from './primitives';
import { useSettings } from '@/contexts/SettingsContext';
import type { TranscriptionProvider } from '@/lib/settings';

/**
 * Voice input, as a section like every other.
 *
 * These rows used to live loose in the panel's own body, which meant the
 * settings surface had eight addressable sections and one that existed only as
 * markup inside the container. A left-hand section list cannot name that one,
 * so it became a component like the rest.
 */
export const TranscriptionSettingsSection: Component = () => {
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

  const inputClass =
    'bg-control border border-hairline rounded px-2 py-1 text-sm text-shell-ink focus:border-primary focus-ring';
  const selectClass = `cru-select ${inputClass} cursor-pointer`;
  const labelClass = 'text-shell-body text-sm';

  return (
    <>
      <SectionHeader title="Transcription" icon={Mic} />

      <tr class="border-b border-hairline">
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
        <tr class="border-b border-hairline">
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

        <tr class="border-b border-hairline">
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

        <tr class="border-b border-hairline">
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
    </>
  );
};
