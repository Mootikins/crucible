// src/lib/settings.ts

/** Scope for settings - local (localStorage) or account (future server sync) */
export type SettingScope = 'local' | 'account';

/** Transcription provider type */
export type TranscriptionProvider = 'local' | 'server';

/** Settings for voice transcription */
export interface TranscriptionSettings {
  /** Which provider to use for transcription */
  provider: TranscriptionProvider;
  /** Server URL for server-based transcription */
  serverUrl: string;
  /** Model name to use */
  model: string;
  /** Language code or 'auto' for automatic detection */
  language: string;
}

/** Root application settings structure */
export interface AppSettings {
  transcription: TranscriptionSettings;
}

/** Default settings values */
export const defaultSettings: AppSettings = {
  transcription: {
    provider: 'local',
    serverUrl: 'https://llama.krohnos.io',
    model: 'whisper-large-v3-turbo',
    language: 'auto',
  },
};

/** localStorage key for persisting settings */
export const SETTINGS_STORAGE_KEY = 'crucible:settings';

/**
 * Load settings from localStorage, merging with defaults for any missing keys.
 * Returns defaultSettings if localStorage is empty or contains invalid JSON.
 */
export function loadSettings(): AppSettings {
  try {
    const stored = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return {
        ...defaultSettings,
        ...parsed,
        transcription: {
          ...defaultSettings.transcription,
          ...parsed.transcription,
        },
      };
    }
  } catch (e) {
    console.warn('Failed to load settings:', e);
  }
  return { transcription: { ...defaultSettings.transcription } };
}

/**
 * Save settings to localStorage.
 */
export function saveSettings(settings: AppSettings): void {
  try {
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    console.error('Failed to save settings:', e);
  }
}
