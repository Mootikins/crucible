// src/contexts/SettingsContext.test.tsx
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createRoot } from 'solid-js';
import { SettingsProvider, useSettings } from './SettingsContext';
import { SETTINGS_STORAGE_KEY } from '@/lib/settings';

describe('SettingsContext', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it('provides default settings when localStorage is empty', () => {
    createRoot((dispose) => {
      const TestComponent = () => {
        const { settings } = useSettings();
        expect(settings.transcription.provider).toBe('local');
        expect(settings.transcription.serverUrl).toBe('https://llama.krohnos.io');
        return null;
      };

      <SettingsProvider>
        <TestComponent />
      </SettingsProvider>;

      dispose();
    });
  });

  it('loads settings from localStorage', () => {
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({
      transcription: { provider: 'server', serverUrl: 'https://custom.url' }
    }));

    createRoot((dispose) => {
      const TestComponent = () => {
        const { settings } = useSettings();
        expect(settings.transcription.provider).toBe('server');
        expect(settings.transcription.serverUrl).toBe('https://custom.url');
        expect(settings.transcription.model).toBe('whisper-large-v3-turbo');
        return null;
      };

      <SettingsProvider>
        <TestComponent />
      </SettingsProvider>;

      dispose();
    });
  });

  it('updateSetting updates the store and persists', () => {
    createRoot((dispose) => {
      const TestComponent = () => {
        const { settings, updateSetting } = useSettings();
        expect(settings.transcription.provider).toBe('local');
        updateSetting('transcription', 'provider', 'server');
        expect(settings.transcription.provider).toBe('server');
        const stored = JSON.parse(localStorage.getItem(SETTINGS_STORAGE_KEY)!);
        expect(stored.transcription.provider).toBe('server');
        return null;
      };

      <SettingsProvider>
        <TestComponent />
      </SettingsProvider>;

      dispose();
    });
  });

  it('resetSettings restores defaults and persists', () => {
    createRoot((dispose) => {
      const TestComponent = () => {
        const { settings, updateSetting, resetSettings } = useSettings();
        updateSetting('transcription', 'provider', 'server');
        expect(settings.transcription.provider).toBe('server');

        // Call resetSettings and check the store and localStorage
        resetSettings();

        // Check localStorage first (this is synchronous and should be updated)
        const stored = JSON.parse(localStorage.getItem(SETTINGS_STORAGE_KEY)!);
        expect(stored.transcription.provider).toBe('local');

        // Then check the reactive store
        expect(settings.transcription.provider).toBe('local');
        return null;
      };

      <SettingsProvider>
        <TestComponent />
      </SettingsProvider>;

      dispose();
    });
  });

  it('throws when useSettings is used outside provider', () => {
    expect(() => {
      createRoot((dispose) => {
        useSettings();
        dispose();
      });
    }).toThrow('useSettings must be used within a SettingsProvider');
  });
});
