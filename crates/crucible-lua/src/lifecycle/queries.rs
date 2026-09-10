use super::PluginManager;
use crate::discovered::{DiscoveredCommand, DiscoveredTool};
use crate::manifest::LoadedPlugin;
#[cfg(any(test, feature = "test-utils"))]
use crate::manifest::PluginState;

impl PluginManager {
    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

    /// Add the capabilities a plugin's returned spec table declared to the
    /// ones its manifest already carries.
    ///
    /// A plugin declares in `plugin.yaml`, in the spec table, or in both, and
    /// the grants it runs under must be the union. Without this the DAEMON
    /// loader read only the manifest, so a plugin that declared its
    /// capabilities the Lua way ran with none of them — and now that the
    /// grants are enforced, that is a refusal rather than a shrug.
    ///
    /// An unrecognised name is a typo in the plugin, not a no-op: the caller
    /// receives it back to report.
    pub fn merge_spec_capabilities<'a>(
        &mut self,
        name: &str,
        declared: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let mut unknown = Vec::new();
        let Some(plugin) = self.plugins.get_mut(name) else {
            return unknown;
        };
        for text in declared {
            match crate::lifecycle::spec::parse_capability(text) {
                Some(cap) => {
                    if !plugin.manifest.capabilities.contains(&cap) {
                        plugin.manifest.capabilities.push(cap);
                    }
                }
                None => unknown.push(text.to_string()),
            }
        }
        unknown
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn active_plugins(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins
            .values()
            .filter(|p| p.state == PluginState::Active)
    }

    pub fn tools(&self) -> Vec<&DiscoveredTool> {
        self.tools.iter().map(|t| &t.item).collect()
    }

    pub fn commands(&self) -> Vec<&DiscoveredCommand> {
        self.commands.iter().map(|c| &c.item).collect()
    }
}
