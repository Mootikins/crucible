import { Component, onMount } from 'solid-js';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { WindowManager } from '@/components/windowing/WindowManager';
import { registerPanels } from '@/lib/register-panels';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '@/lib/layout-persistence';

const App: Component = () => {
  onMount(() => {
    registerPanels();
    loadLayoutOnStartup();
    setupLayoutAutoSave();
  });
  return (
    <SettingsProvider>
      <WindowManager />
    </SettingsProvider>
  );
};

export default App;
