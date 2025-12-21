import { createSignal, createEffect } from 'solid-js'

const THEME_STORAGE_KEY = 'crucible-theme'
const ACCENT_HUE_KEY = 'crucible-accent-hue'

function createThemeStore() {
  // Initialize from localStorage or defaults
  const getInitialDarkMode = () => {
    if (typeof window === 'undefined') return false
    const stored = localStorage.getItem(THEME_STORAGE_KEY)
    if (stored === 'dark' || stored === 'light') return stored === 'dark'
    // Default to dark mode
    return true
  }

  const getInitialAccentHue = () => {
    if (typeof window === 'undefined') return 220
    const stored = localStorage.getItem(ACCENT_HUE_KEY)
    if (stored) {
      const hue = parseInt(stored, 10)
      if (!isNaN(hue) && hue >= 0 && hue <= 360) return hue
    }
    return 220 // Default blue
  }

  const [isDark, setIsDark] = createSignal(getInitialDarkMode())
  const [accentHue, setAccentHue] = createSignal(getInitialAccentHue())

  // Apply dark mode class to document
  createEffect(() => {
    if (typeof document !== 'undefined') {
      if (isDark()) {
        document.documentElement.classList.add('dark')
      } else {
        document.documentElement.classList.remove('dark')
      }
    }
  })

  // Persist dark mode
  createEffect(() => {
    if (typeof window !== 'undefined') {
      localStorage.setItem(THEME_STORAGE_KEY, isDark() ? 'dark' : 'light')
    }
  })

  // Apply accent hue to CSS variable
  createEffect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.style.setProperty('--accent-hue', accentHue().toString())
    }
  })

  // Persist accent hue
  createEffect(() => {
    if (typeof window !== 'undefined') {
      localStorage.setItem(ACCENT_HUE_KEY, accentHue().toString())
    }
  })

  return {
    isDark,
    setIsDark,
    toggleDarkMode: () => setIsDark(!isDark()),
    accentHue,
    setAccentHue: (hue: number) => {
      if (hue >= 0 && hue <= 360) {
        setAccentHue(hue)
      }
    },
  }
}

export const themeStore = createThemeStore()

