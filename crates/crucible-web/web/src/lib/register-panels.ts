import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
  registry.register('chat', 'Chat', ChatPanel, 'center', '💬');
}
