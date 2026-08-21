import { getGlobalRegistry } from './panel-registry';
import { SettingsPanel } from '@/components/SettingsPanel';
import { ChatPanel } from '@/components/ChatPanel';
import { CenterComposer } from '@/components/CenterComposer';
import { ActivityPanel } from '@/components/ActivityPanel';
import { TerminalPanel } from '@/components/TerminalPanel';
import { SessionsPanel } from '@/components/SessionsPanel';
import { FilesPanel } from '@/components/FilesPanel';
import { SearchPanel } from '@/components/SearchPanel';
import { SkillsPanel } from '@/components/SkillsPanel';
import { PluginPanel } from '@/components/PluginPanel';
import FileViewerPanel from '@/components/FileViewerPanel';
import InboxPanel from '@/components/InboxPanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { ChangesPanel } from '@/components/ChangesPanel';
import { GraphPanel } from '@/components/graph/GraphPanel';
import { CanvasPanel } from '@/components/canvas/CanvasPanel';

// Tab/ribbon icons are NOT registered here — they resolve per content type
// through lib/tab-icons.ts (SVG components, consistent monochrome chrome).
export function registerPanels(): void {
  const registry = getGlobalRegistry();
  // Identity on the left, working context on the right. Sessions and the file
  // tree are separate surfaces so that neither hides the other: the Navigator
  // made them two scopes of one panel, which meant reading a file cost you
  // sight of the session list.
  registry.register('sessions', 'Sessions', SessionsPanel, 'left');
  registry.register('files', 'Files', FilesPanel, 'right');
  // Search belongs to NEITHER rail: it searches files, notes AND sessions,
  // and carries its own scope menu. Docking it beside the session list would
  // claim a scope it does not have, and a ~250px rail is too narrow for
  // path:line hits anyway. It is registered but never seeded — `openPanelTab`
  // focuses an existing tab wherever the user docked it, and opens a new one
  // in the center, where results have room.
  registry.register('search', 'Search', SearchPanel, 'center');
  registry.register('settings', 'Settings', SettingsPanel, 'center');
  registry.register('chat', 'Chat', ChatPanel, 'center');
  registry.register('chat-draft', 'New Session', CenterComposer, 'center');
  registry.register('inbox', 'Inbox', InboxPanel, 'center');
  registry.register('activity', 'Activity', ActivityPanel, 'right');
  registry.register('terminal', 'Terminal', TerminalPanel, 'bottom');
  registry.register('file', 'File', FileViewerPanel, 'center');
  registry.register('skills', 'Skills', SkillsPanel, 'left');
  registry.register('plugins', 'Plugins', PluginPanel, 'left');
  registry.register('backlinks', 'Backlinks', BacklinksPanel, 'right');
  // The session's review queue, beside Activity and Backlinks. Reviewing
  // happens in the center buffer; this is the index into it.
  registry.register('changes', 'Changes', ChangesPanel, 'right');
  registry.register('graph', 'Graph', GraphPanel, 'center');
  registry.register('canvas', 'Canvas', CanvasPanel, 'center');
}
