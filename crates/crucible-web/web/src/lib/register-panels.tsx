import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { ShellPanel } from '@/components/ShellPanel';
import { SessionPanel } from '@/components/SessionPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import PlaceholderPanel from '@/components/PlaceholderPanel';

// Wrapper components for placeholder panels with specific names
function ExplorerPanel() { return <PlaceholderPanel name="Explorer" />; }
function SearchPanel() { return <PlaceholderPanel name="Search" />; }
function SourceControlPanel() { return <PlaceholderPanel name="Source Control" />; }
function OutlinePanel() { return <PlaceholderPanel name="Outline" />; }
function ProblemsPanel() { return <PlaceholderPanel name="Problems" />; }
function OutputPanel() { return <PlaceholderPanel name="Output" />; }

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
