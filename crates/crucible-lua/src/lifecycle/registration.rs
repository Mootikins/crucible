use super::PluginManager;

/// A spec export tagged with the plugin that declared it, so `forget` can
/// drop everything a plugin registered in one pass.
#[derive(Debug, Clone)]
pub(super) struct RegisteredItem<T> {
    pub(super) item: T,
    pub(super) owner: Option<String>,
}

impl PluginManager {
    pub fn unregister_by_owner(&mut self, owner: &str) -> usize {
        let before = self.tools.len() + self.commands.len();

        let matches_owner =
            |item_owner: &Option<String>| item_owner.as_ref().is_some_and(|o| o == owner);

        self.tools.retain(|t| !matches_owner(&t.owner));
        self.commands.retain(|c| !matches_owner(&c.owner));

        let after = self.tools.len() + self.commands.len();
        before - after
    }
}
