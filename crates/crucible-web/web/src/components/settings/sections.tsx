import { Component } from 'solid-js';
import {
  Cog,
  Cpu,
  Key,
  LayoutDashboard,
  Mic,
  Package,
  Palette,
  Pencil,
  Plug,
  Terminal,
} from '@/lib/icons';
import {
  ApiAccessSection,
  AppearanceSettingsSection,
  EditorSettingsSection,
  McpStatusSection,
  ModelSettingsSection,
  PluginsSection,
  TerminalSettingsSection,
  TranscriptionSettingsSection,
} from '@/components/SettingsPanel';
import { AdvancedSessionSettingsSection } from './AdvancedSessionSettings';
import { AppConfigSettingsSection } from './AppConfigSettings';
import { PluginSettings } from '@/components/PluginSettings';
import type { PluginOptionNode } from '@/lib/api';
import { WorkspaceSettingsSection } from './WorkspaceSettings';

/** A plugin's declared settings tree, as the daemon describes it. */
export type PluginTrees = Record<string, PluginOptionNode>;

/**
 * What every section may be handed, whether or not it uses it.
 *
 * Both are OPTIONAL and both are ignored by most sections. A prop rather than
 * a per-section wiring, because the registry renders through `<Dynamic>`: a
 * section that needs to reload the plugin trees, or to dismiss the dialog it
 * sits in, cannot reach either from inside itself.
 */
export interface SettingsSectionProps {
  /** Re-read what the surrounding dialog fetched; resolves when it is in hand. */
  onChanged?: () => void | Promise<unknown>;
  /** Dismiss the dialog. A section that opens a file behind it needs this. */
  onClose?: () => void;
}

export interface SettingsSection {
  id: string;
  /** Left-list label. Short — the body carries the full heading. */
  label: string;
  icon: Component<{ class?: string }>;
  /** Left-list grouping, in declaration order. */
  group: string;
  render: Component<SettingsSectionProps>;
}

let cached: SettingsSection[] | null = null;

/**
 * Every settings section, in the order the left list shows them.
 *
 * ONE table, and it is the only place a section is declared. Both doorways
 * read it — the modal renders one section at a time, and the legacy settings
 * TAB renders all of them stacked — so a new section appears in both without
 * being written twice, and the two can never drift into showing different
 * settings.
 *
 * Grouped the way the user's own mental model runs: what the app looks like,
 * what the agent does, what it is connected to, and the workspace itself.
 *
 * A FUNCTION, not a module-level array, and that is load-bearing. This module
 * imports its section components from `SettingsPanel.tsx`, which imports this
 * table back to render the stacked tab. A top-level array evaluates those
 * `const` bindings while the other module is still initialising, so whichever
 * side the bundler happened to evaluate first threw
 * `ReferenceError: Cannot access 'AppearanceSettingsSection' before
 * initialization` — and took the whole app down at boot, with every unit test
 * still green, because no test imports both modules in that order. Building
 * on first CALL moves the reads past both modules' initialisation.
 */
function builtins(): SettingsSection[] {
  if (cached) return cached;
  cached = [
    { id: 'appearance', label: 'Appearance', icon: Palette, group: 'Look and feel', render: AppearanceSettingsSection },
    { id: 'editor', label: 'Editor', icon: Pencil, group: 'Look and feel', render: EditorSettingsSection },
    { id: 'terminal', label: 'Terminal', icon: Terminal, group: 'Look and feel', render: TerminalSettingsSection },

    { id: 'model', label: 'Model', icon: Cpu, group: 'Agent', render: ModelSettingsSection },
    { id: 'advanced-session', label: 'Advanced session', icon: LayoutDashboard, group: 'Agent', render: AdvancedSessionSettingsSection },
    { id: 'transcription', label: 'Voice input', icon: Mic, group: 'Agent', render: TranscriptionSettingsSection },

    { id: 'api-access', label: 'API access', icon: Key, group: 'Connections', render: ApiAccessSection },
    { id: 'plugins', label: 'Plugins', icon: Package, group: 'Connections', render: PluginsSection },
    { id: 'mcp', label: 'MCP', icon: Plug, group: 'Connections', render: McpStatusSection },

    { id: 'workspace', label: 'Workspace', icon: LayoutDashboard, group: 'Workspace', render: WorkspaceSettingsSection },

    // The DAEMON's config, beside the browser-local sections above rather than
    // replacing any of them: fonts, terminal size, vim mode and the microphone
    // are per-device and stay in localStorage.
    { id: 'app-config', label: 'Configuration', icon: Cog, group: 'Crucible', render: AppConfigSettingsSection },
  ];
  return cached;
}

/**
 * Every section, including one per plugin that declared a settings tree.
 *
 * A plugin section is DATA, not code: the label is the plugin's own name and
 * the body is the tree the daemon sent. There is no per-plugin branch here and
 * no list of known plugins — the same rule the session status chips carry. A
 * plugin shipped tomorrow gets its own entry in the left list for free, which
 * is the entire reason its settings are declared in Lua and projected.
 *
 * Plugins that declare an EMPTY tree are dropped rather than given an entry
 * that opens onto nothing.
 *
 * `trees` is optional so the stacked tab and every test can call this with no
 * daemon behind them.
 */
export function settingsSections(trees?: PluginTrees, onChanged?: () => void | Promise<unknown>): SettingsSection[] {
  const plugins = Object.entries(trees ?? {})
    .filter(([, tree]) => (tree?.args ?? []).length > 0)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([plugin, tree]): SettingsSection => ({
      // Namespaced so a plugin called "editor" cannot collide with the app's
      // own section ids, which the left list keys on.
      id: `plugin:${plugin}`,
      label: tree.name ?? plugin,
      icon: Package,
      group: 'Plugins',
      render: () => (
        <PluginSettings
          plugin={plugin}
          tree={tree}
          onChanged={async () => {
            await onChanged?.();
          }}
        />
      ),
    }));
  return [...builtins(), ...plugins];
}

/** The groups, in declaration order, each with its own sections. */
export function settingsGroups(
  sections: SettingsSection[],
): { group: string; sections: SettingsSection[] }[] {
  const out: { group: string; sections: SettingsSection[] }[] = [];
  for (const section of sections) {
    const last = out[out.length - 1];
    if (last && last.group === section.group) last.sections.push(section);
    else out.push({ group: section.group, sections: [section] });
  }
  return out;
}
