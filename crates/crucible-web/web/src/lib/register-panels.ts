import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { ShellPanel } from '@/components/ShellPanel';
import { SessionPanel } from '@/components/SessionPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import PlaceholderPanel from '@/components/PlaceholderPanel';

// Wrapper components for placeholder panels with specific names
const ExplorerPanel = () => PlaceholderPanel({ name: 'Explorer' });
const SearchPanel = () => PlaceholderPanel({ name: 'Search' });
const SourceControlPanel = () => PlaceholderPanel({ name: 'Source Control' });
const OutlinePanel = () => PlaceholderPanel({ name: 'Outline' });
const ProblemsPanel = () => PlaceholderPanel({ name: 'Problems' });
const OutputPanel = () => PlaceholderPanel({ name: 'Output' });

export function registerPanels(): void {
  const registry = getGlobalRegistry();
  registry.register('sessions', 'Sessions', SessionPanel, 'left', '📋');
  registry.register('settings', 'Settings', SettingsPanel, 'center', '⚙️');
  registry.register('chat', 'Chat', ChatPanel, 'center', '💬');
  registry.register('activity', 'Activity', ActivityPanel, 'right', '📊');
  registry.register('terminal', 'Terminal', ShellPanel, 'bottom', '💻');
  registry.register('file', 'File', FileViewerPanel, 'center', '📄');
  registry.register('explorer', 'Explorer', ExplorerPanel, 'left', '🗂️');
  registry.register('search', 'Search', SearchPanel, 'left', '🔍');
  registry.register('source-control', 'Source Control', SourceControlPanel, 'left', '🌿');
  registry.register('outline', 'Outline', OutlinePanel, 'right', '📑');
  registry.register('problems', 'Problems', ProblemsPanel, 'bottom', '⚠️');
  registry.register('output', 'Output', OutputPanel, 'bottom', '📤');
}
