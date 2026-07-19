// src/contexts/SettingsContext.tsx
import {
  createContext,
  useContext,
  createEffect,
  ParentComponent,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import {
  AppSettings,
  defaultSettings,
  loadSettings,
  saveSettings,
} from '@/lib/settings';

/** Context value type for settings */
export interface SettingsContextValue {
  /** Current settings (reactive store) */
  settings: AppSettings;
  /** Update a single setting value */
  updateSetting: <K extends keyof AppSettings>(
    section: K,
    key: keyof AppSettings[K],
    value: AppSettings[K][keyof AppSettings[K]]
  ) => void;
  /** Reset all settings to defaults */
  resetSettings: () => void;
}

const SettingsContext = createContext<SettingsContextValue>();

/**
 * Provider component that manages application settings.
 * Loads settings from localStorage on mount and persists changes automatically.
 */
export const SettingsProvider: ParentComponent = (props) => {
  const [settings, setSettings] = createStore<AppSettings>(loadSettings());

  // Apply the chosen fonts by overriding the --font-sans/--font-mono CSS vars
  // (defined in index.css @theme). Empty setting = remove the override so the
  // built-in IBM Plex default applies. Reactive: re-runs when the setting changes.
  createEffect(() => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    const sans = settings.appearance.fontSans.trim();
    const mono = settings.appearance.fontMono.trim();
    if (sans) root.style.setProperty('--font-sans', sans);
    else root.style.removeProperty('--font-sans');
    if (mono) root.style.setProperty('--font-mono', mono);
    else root.style.removeProperty('--font-mono');
  });

  const updateSetting = <K extends keyof AppSettings>(
    section: K,
    key: keyof AppSettings[K],
    value: AppSettings[K][keyof AppSettings[K]]
  ) => {
    setSettings(
      produce((s) => {
        (s[section] as unknown as Record<string, unknown>)[key as string] = value;
      })
    );
    // Create a snapshot of the current settings for persistence
    const snapshot = {
      ...settings,
      [section]: {
        ...settings[section],
        [key]: value,
      },
    };
    saveSettings(snapshot as AppSettings);
  };

  const resetSettings = () => {
    setSettings(produce((s) => {
      s.transcription = { ...defaultSettings.transcription };
    }));
    saveSettings({ ...defaultSettings });
  };

  const value: SettingsContextValue = {
    settings,
    updateSetting,
    resetSettings,
  };

  return (
    <SettingsContext.Provider value={value}>
      {props.children}
    </SettingsContext.Provider>
  );
};

/**
 * Hook to access settings context.
 * Must be used within a SettingsProvider.
 * @throws Error if used outside of SettingsProvider
 */
export function useSettings(): SettingsContextValue {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error('useSettings must be used within a SettingsProvider');
  }
  return context;
}

/**
 * Settings access with a defaults fallback for contexts mounted without a
 * SettingsProvider (test harnesses); writes are no-ops there.
 */
export function useSettingsSafe(): SettingsContextValue {
  const context = useContext(SettingsContext);
  return (
    context ?? {
      settings: defaultSettings,
      updateSetting: () => {},
      resetSettings: () => {},
    }
  );
}
