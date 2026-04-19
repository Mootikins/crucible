use super::PluginManager;
use crate::annotations::{DiscoveredCommand, DiscoveredHandler, DiscoveredTool, DiscoveredView};
use crate::manifest::{Capability, LoadedPlugin, PluginState};

impl PluginManager {
    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

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

    pub fn views(&self) -> Vec<&DiscoveredView> {
        self.views.iter().map(|v| &v.item).collect()
    }

    pub fn handlers(&self) -> Vec<&DiscoveredHandler> {
        self.handlers.iter().map(|h| &h.item).collect()
    }

    pub fn plugin_has_capability(&self, name: &str, cap: Capability) -> bool {
        self.plugins
            .get(name)
            .is_some_and(|p| p.manifest.has_capability(cap))
    }

    pub fn load_errors(&self) -> Vec<(&str, &str)> {
        self.plugins
            .iter()
            .filter(|(_, p)| p.state == PluginState::Error)
            .filter_map(|(name, p)| p.last_error.as_deref().map(|e| (name.as_str(), e)))
            .collect()
    }
}
