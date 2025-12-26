// src/lib/settings.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  loadSettings,
  saveSettings,
  defaultSettings,
  SETTINGS_STORAGE_KEY,
} from './settings';

describe('settings', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  describe('defaultSettings', () => {
    it('has local as default provider', () => {
      expect(defaultSettings.transcription.provider).toBe('local');
    });

    it('has llama.krohnos.io as default server URL', () => {
      expect(defaultSettings.transcription.serverUrl).toBe('https://llama.krohnos.io');
    });

    it('has whisper-large-v3-turbo as default model', () => {
      expect(defaultSettings.transcription.model).toBe('whisper-large-v3-turbo');
    });

    it('has auto as default language', () => {
      expect(defaultSettings.transcription.language).toBe('auto');
    });
  });

  describe('loadSettings', () => {
    it('returns defaults when localStorage is empty', () => {
      const settings = loadSettings();
      expect(settings).toEqual(defaultSettings);
    });

    it('loads settings from localStorage', () => {
      localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({
        transcription: { provider: 'server', serverUrl: 'https://custom.url' }
      }));

      const settings = loadSettings();
      expect(settings.transcription.provider).toBe('server');
      expect(settings.transcription.serverUrl).toBe('https://custom.url');
    });

    it('merges with defaults for missing keys', () => {
      localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({
        transcription: { provider: 'server' }
      }));

      const settings = loadSettings();
      expect(settings.transcription.provider).toBe('server');
      expect(settings.transcription.model).toBe('whisper-large-v3-turbo');
    });

    it('returns defaults on invalid JSON', () => {
      localStorage.setItem(SETTINGS_STORAGE_KEY, 'not valid json');
      const settings = loadSettings();
      expect(settings).toEqual(defaultSettings);
    });
  });

  describe('saveSettings', () => {
    it('persists settings to localStorage', () => {
      const settings = {
        ...defaultSettings,
        transcription: {
          ...defaultSettings.transcription,
          provider: 'server' as const,
        },
      };

      saveSettings(settings);

      const stored = JSON.parse(localStorage.getItem(SETTINGS_STORAGE_KEY)!);
      expect(stored.transcription.provider).toBe('server');
    });
  });
});
