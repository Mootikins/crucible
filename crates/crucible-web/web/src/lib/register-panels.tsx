import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { DraftSessionPanel } from '@/components/DraftSessionPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { TerminalPanel } from '@/components/TerminalPanel';
import { NavigatorPanel } from '@/components/NavigatorPanel';
import { SkillsPanel } from '@/components/SkillsPanel';
import { PluginPanel } from '@/components/PluginPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import InboxPanel from '@/components/InboxPanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { GraphPanel } from '@/components/graph/GraphPanel';

// Tab/ribbon icons are NOT registered here — they resolve per content type
// through lib/tab-icons.ts (SVG components, consistent monochrome chrome).
export function registerPanels(): void {
  const registry = getGlobalRegistry();
  // The Navigator replaces the separate Files / Sessions / Search left tabs:
  // one panel with a scope swapper (sessions / kilns / projects) + search.
  registry.register('navigator', 'Navigator', NavigatorPanel, 'left');
  registry.register('settings', 'Settings', SettingsPanel, 'center');
  registry.register('chat', 'Chat', ChatPanel, 'center');
  registry.register('chat-draft', 'New Session', DraftSessionPanel, 'center');
  registry.register('inbox', 'Inbox', InboxPanel, 'center');
  registry.register('activity', 'Activity', ActivityPanel, 'right');
  registry.register('terminal', 'Terminal', TerminalPanel, 'bottom');
  registry.register('file', 'File', FileViewerPanel, 'center');
  registry.register('skills', 'Skills', SkillsPanel, 'left');
  registry.register('plugins', 'Plugins', PluginPanel, 'left');
  registry.register('backlinks', 'Backlinks', BacklinksPanel, 'right');
  registry.register('graph', 'Graph', GraphPanel, 'center');
}
