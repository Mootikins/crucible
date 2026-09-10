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
