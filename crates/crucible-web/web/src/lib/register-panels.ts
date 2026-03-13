import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { ShellPanel } from '@/components/ShellPanel';
import { SessionPanel } from '@/components/SessionPanel';
import FileViewerPanel from '@/components/FileViewerPanel';

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left', '📋');
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
  registry.register('chat', 'Chat', ChatPanel, 'center', '💬');
  registry.register('activity', 'Activity', ActivityPanel, 'right', '📊');
  registry.register('terminal', 'Terminal', ShellPanel, 'bottom', '💻');
  registry.register('file', 'File', FileViewerPanel, 'center', '📄');
}
