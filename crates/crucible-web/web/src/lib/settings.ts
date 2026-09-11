// src/lib/settings.ts


/** Transcription provider type */
export type TranscriptionProvider = 'local' | 'server';

/** Settings for voice transcription */
interface TranscriptionSettings {
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
interface EditorSettings {
  /** Modal vim keybindings (@replit/codemirror-vim) */
  vimMode: boolean;
  /** Save a NOTE — a file inside a kiln — this many idle seconds after its
   * last edit (0 = off). A project file always saves by hand: autosaving code
   * would fire watchers and builds mid-edit. */
  autosaveSeconds: number;
  /** Readable line length for editing/reading views, px (0 = full width). */
  maxLineWidth: number;
  /** What hover popovers open as: rendered reading view, live preview, or
   * raw source. */
  hoverMode: 'reading' | 'live' | 'source';
  /** Show the save affordance (dirty dot + Save) in the status bar. */
  showSaveButton: boolean;
  /** Render `$…$`/`$$…$$` math as KaTeX in live preview (off = raw source). */
  renderMath: boolean;
  /** Render ```mermaid fences as diagrams in live preview (off = raw source). */
  renderDiagrams: boolean;
  /**
   * Hide the blank lines between frontmatter and the first content line, in
   * live preview.
   *
   * Only live preview: the reading view renders markdown, which discards
   * leading blank lines already, so there is nothing there to hide.
   */
  hideFrontmatterGap: boolean;
}

/** Appearance / typography settings */
interface AppearanceSettings {
  /** CSS font-family for UI + prose text. Empty = built-in default (Geist). */
  fontSans: string;
  /** CSS font-family for code / monospace. Empty = built-in default (Geist Mono). */
  fontMono: string;
}

/** Settings for the xterm terminal panel. xterm renders to canvas and can't
 * read CSS vars, so its font is a real setting rather than a stylesheet. */
interface TerminalSettings {
  /** CSS font-family for the terminal. Empty = follow the Appearance code font. */
  fontFamily: string;
  /** Terminal font size in px. */
  fontSize: number;
}

/**
 * The stored settings' shape version. Bump it with a migration in
 * `loadSettings` when a default changes meaning.
 *
 * 2 — notes autosave by default. Version 1 stored `autosaveSeconds: 0`, the old
 * default, in every browser that ever saved a setting, so a new default alone
 * would have reached no existing install.
 */
export const SETTINGS_VERSION = 2;

/** Root application settings structure */
export interface AppSettings {
  version: number;
  transcription: TranscriptionSettings;
  editor: EditorSettings;
  appearance: AppearanceSettings;
  terminal: TerminalSettings;
}

/** The settings a user edits: every section, never the stored `version`. */
export type SettingsSection = Exclude<keyof AppSettings, 'version'>;

/** Default settings values */
export const defaultSettings: AppSettings = {
  version: SETTINGS_VERSION,
  transcription: {
    provider: 'local',
    serverUrl: '',
    model: 'whisper-large-v3-turbo',
    language: 'auto',
  },
  editor: {
    vimMode: true,
    autosaveSeconds: 2,
    showSaveButton: true,
    // Matches the reading view's prose column (max-w-3xl).
    maxLineWidth: 768,
    hoverMode: 'reading',
    renderMath: true,
    renderDiagrams: true,
    hideFrontmatterGap: true,
  },
  // Empty = use the built-in @theme defaults (Geist) from index.css.
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
      const merged: AppSettings = {
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
      return migrate(merged, typeof parsed.version === 'number' ? parsed.version : 1);
    }
  } catch (e) {
    console.warn('Failed to load settings:', e);
  }
  return {
    version: SETTINGS_VERSION,
    transcription: { ...defaultSettings.transcription },
    editor: { ...defaultSettings.editor },
    appearance: { ...defaultSettings.appearance },
    terminal: { ...defaultSettings.terminal },
  };
}

/** Bring settings stored at `from` up to `SETTINGS_VERSION`. */
function migrate(settings: AppSettings, from: number): AppSettings {
  if (from < 2 && settings.editor.autosaveSeconds === 0) {
    // A stored 0 at version 1 is almost always the old default, saved along
    // with some unrelated setting. A user who wanted it off turns it off once
    // more; that choice is stored at version 2 and kept.
    settings.editor.autosaveSeconds = defaultSettings.editor.autosaveSeconds;
  }
  settings.version = SETTINGS_VERSION;
  return settings;
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
