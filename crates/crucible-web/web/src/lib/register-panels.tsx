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
import { ConflictsPanel } from '@/components/ConflictsPanel';
import { GraphPanel } from '@/components/graph/GraphPanel';
import { CanvasPanel } from '@/components/canvas/CanvasPanel';
import { PluginBlockPanel } from '@/components/blocks/PluginBlockPanel';
import { SurfacesPanel } from '@/components/SurfacesPanel';

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
  registry.register('terminal', 'Terminal', TerminalPanel, 'right');
  registry.register('file', 'File', FileViewerPanel, 'center');
  registry.register('skills', 'Skills', SkillsPanel, 'left');
  registry.register('plugins', 'Plugins', PluginPanel, 'left');
  registry.register('backlinks', 'Backlinks', BacklinksPanel, 'right');
  // The session's review queue, beside Activity and Backlinks. Reviewing
  // happens in the center buffer; this is the index into it.
  registry.register('changes', 'Changes', ChangesPanel, 'right');
  // A note write waiting on a person. Centre, not the right rail: settling one
  // is reading and choosing inside the note, which is editor work, and the
  // phone opens it as a content tab from the same registry.
  registry.register('conflicts', 'Conflicts', ConflictsPanel, 'center');
  registry.register('graph', 'Graph', GraphPanel, 'center');
  registry.register('canvas', 'Canvas', CanvasPanel, 'center');
  // A plugin block, docked rather than embedded in a note. Until this existed
  // a plugin could contribute content to a document and could not contribute a
  // panel, which blocked rebuilding any existing panel as a plugin.
  registry.register('plugin-blocks', 'Plugin Blocks', PluginBlockPanel, 'right');
  // Panels plugins declared. Registered but not seeded: a surface exists only
  // once some plugin declares one, so seeding an empty rail would advertise a
  // feature the box may not have.
  registry.register('surfaces', 'Surfaces', SurfacesPanel, 'left');
}
