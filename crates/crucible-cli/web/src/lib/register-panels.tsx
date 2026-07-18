import { lazy, Suspense, type Component } from 'solid-js';
import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { ActivityPanel } from '@/components/ActivityPanel';
import { TerminalPanel } from '@/components/TerminalPanel';
import { SessionPanel } from '@/components/SessionPanel';
import { SkillsPanel } from '@/components/SkillsPanel';
import { PluginPanel } from '@/components/PluginPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import InboxPanel from '@/components/InboxPanel';
import HomePanel from '@/components/HomePanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';

// Lazy-load the file-tree panel: it pulls in the heavy @ark-ui/@zag-js TreeView
// and Menu state machines (plus @zag-js/collection). Keeping them out of the
// initial eager module graph keeps first paint light — and, in the Playwright
// dev-server, avoids delaying webfont application, which otherwise causes FOUT
// (fallback-font capture) in unrelated screenshot stories. Suspense is wrapped
// here so the <Dynamic> panel render sites need no changes.
const LazyFilesPanel = lazy(() =>
  import('@/components/FilesPanel').then((m) => ({ default: m.FilesPanel })),
);
const FilesPanel: Component = (props) => (
  <Suspense fallback={null}>
    <LazyFilesPanel {...props} />
  </Suspense>
);

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
}
