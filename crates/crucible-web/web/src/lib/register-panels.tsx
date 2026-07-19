import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { TerminalPanel } from '@/components/TerminalPanel';
import { SessionPanel } from '@/components/SessionPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { SkillsPanel } from '@/components/SkillsPanel';
import { PluginPanel } from '@/components/PluginPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import InboxPanel from '@/components/InboxPanel';
import HomePanel from '@/components/HomePanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { GraphPanel } from '@/components/graph/GraphPanel';

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left', '📋');
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
  registry.register('chat', 'Chat', ChatPanel, 'center', '💬');
  registry.register('inbox', 'Inbox', InboxPanel, 'center', '📥');
  registry.register('home', 'Home', HomePanel, 'center', '⌂');
  registry.register('activity', 'Activity', ActivityPanel, 'right', '📊');
  registry.register('terminal', 'Terminal', TerminalPanel, 'bottom', '💻');
  registry.register('file', 'File', FileViewerPanel, 'center', '📄');
  registry.register('files', 'Files', FilesPanel, 'left', '📁');
  registry.register('skills', 'Skills', SkillsPanel, 'left', '🎯');
  registry.register('plugins', 'Plugins', PluginPanel, 'left', '🔌');
  registry.register('backlinks', 'Backlinks', BacklinksPanel, 'right', '🔗');
  registry.register('graph', 'Graph', GraphPanel, 'center', '🕸️');
}
