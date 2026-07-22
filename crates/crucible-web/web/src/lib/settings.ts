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

/** Settings for the CodeMirror note/file editor */
export interface EditorSettings {
  /** Modal vim keybindings (@replit/codemirror-vim) */
  vimMode: boolean;
  /** Autosave dirty buffers after this many idle seconds (0 = off). */
  autosaveSeconds: number;
  /** Readable line length for editing/reading views, px (0 = full width). */
  maxLineWidth: number;
  /** What hover popovers open as: rendered reading view, live preview, or
   * raw source. */
  hoverMode: 'reading' | 'live' | 'source';
  /** Show the save affordance (dirty dot + Save) in the status bar. */
  showSaveButton: boolean;
}

/** Appearance / typography settings */
export interface AppearanceSettings {
  /** CSS font-family for UI + prose text. Empty = built-in default (IBM Plex Sans). */
  fontSans: string;
  /** CSS font-family for code / monospace. Empty = built-in default (IBM Plex Mono). */
  fontMono: string;
}

/** Settings for the xterm terminal panel. xterm renders to canvas and can't
 * read CSS vars, so its font is a real setting rather than a stylesheet. */
export interface TerminalSettings {
  /** CSS font-family for the terminal. Empty = follow the Appearance code font. */
  fontFamily: string;
  /** Terminal font size in px. */
  fontSize: number;
}

/** Root application settings structure */
export interface AppSettings {
  transcription: TranscriptionSettings;
  editor: EditorSettings;
  appearance: AppearanceSettings;
  terminal: TerminalSettings;
}

/** Default settings values */
export const defaultSettings: AppSettings = {
  transcription: {
    provider: 'local',
    serverUrl: '',
    model: 'whisper-large-v3-turbo',
    language: 'auto',
  },
  editor: {
    vimMode: true,
    autosaveSeconds: 0,
    showSaveButton: true,
    // Matches the reading view's prose column (max-w-3xl).
    maxLineWidth: 768,
    hoverMode: 'reading',
  },
  // Empty = use the built-in @theme defaults (IBM Plex) from index.css.
  appearance: {
    fontSans: '',
    fontMono: '',
  },
  terminal: {
    fontFamily: '',
    fontSize: 13,
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
        editor: {
          ...defaultSettings.editor,
          ...parsed.editor,
        },
        appearance: {
          ...defaultSettings.appearance,
          ...parsed.appearance,
        },
        terminal: {
          ...defaultSettings.terminal,
          ...parsed.terminal,
        },
      };
    }
  } catch (e) {
    console.warn('Failed to load settings:', e);
  }
  return {
    transcription: { ...defaultSettings.transcription },
    editor: { ...defaultSettings.editor },
    appearance: { ...defaultSettings.appearance },
    terminal: { ...defaultSettings.terminal },
  };
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
