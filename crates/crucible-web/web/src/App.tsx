import { AppLayout } from '~/lib/components/layout/AppLayout'
import { themeStore } from '~/lib/stores/theme'
import { onMount } from 'solid-js'

function App() {
  // Initialize theme on mount
  onMount(() => {
    // Theme is already initialized in the store
    // This ensures the dark class is applied
    const isDark = themeStore.isDark()
    if (isDark) {
      document.documentElement.classList.add('dark')
    }
  })

  return <AppLayout />
}

export default App;

