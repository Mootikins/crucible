import { createSignal, createEffect } from 'solid-js'
import type { LayoutState, PaneConfig } from '../utils/pane-types'
import { loadLayout, saveLayout, getDefaultLayout } from '../utils/layout-persistence'

function createLayoutStore() {
  // Only access localStorage in browser
  let initialState: LayoutState
  if (typeof window !== 'undefined') {
    const stored = loadLayout()
    initialState = stored || getDefaultLayout()
  } else {
    initialState = getDefaultLayout()
  }

  const [layout, setLayout] = createSignal<LayoutState>(initialState)

  // Persist to localStorage whenever layout changes
  createEffect(() => {
    if (typeof window !== 'undefined') {
      saveLayout(layout())
    }
  })

  return {
    layout,
    setLayout: (value: LayoutState) => {
      setLayout(value)
    },
    updateLayout: (fn: (value: LayoutState) => LayoutState) => {
      setLayout(fn(layout()))
    },
    setSidebarWidth: (width: number) => {
      setLayout((prev) => ({
        ...prev,
        sidebarWidth: Math.max(150, Math.min(500, width)), // Clamp between 150-500px
      }))
    },
    toggleSidebar: () => {
      setLayout((prev) => ({
        ...prev,
        sidebarVisible: !prev.sidebarVisible,
      }))
    },
    setActivePane: (paneId: string | null) => {
      setLayout((prev) => ({
        ...prev,
        activePane: paneId,
      }))
    },
    updatePane: (paneId: string, updates: Partial<PaneConfig>) => {
      setLayout((prev) => ({
        ...prev,
        panes: prev.panes.map((p) => (p.id === paneId ? { ...p, ...updates } : p)),
      }))
    },
  }
}

export const layoutStore = createLayoutStore()
