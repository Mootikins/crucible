import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
}
