import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { ShellPanel } from '@/components/ShellPanel';

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
  registry.register('chat', 'Chat', ChatPanel, 'center', '💬');
  registry.register('activity', 'Activity', ActivityPanel, 'right', '📊');
  registry.register('shell', 'Shell', ShellPanel, 'bottom', '💻');
}
