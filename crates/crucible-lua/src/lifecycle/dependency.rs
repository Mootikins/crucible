use super::{LifecycleError, LifecycleResult, PluginManager};
use std::collections::HashMap;

impl PluginManager {
    pub(super) fn resolve_load_order(&self, names: &[String]) -> LifecycleResult<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashMap::new();

        for name in names {
            self.visit_for_order(name, &mut visited, &mut order)?;
        }

        Ok(order)
    }

    pub(super) fn visit_for_order(
        &self,
        name: &str,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) -> LifecycleResult<()> {
        match visited.get(name) {
            Some(true) => return Ok(()),
            Some(false) => {
                return Err(LifecycleError::CircularDependency(name.to_string()));
            }
            None => {}
        }

        visited.insert(name.to_string(), false);

        if let Some(plugin) = self.plugins.get(name) {
            for dep in &plugin.manifest.dependencies {
                if !dep.optional && self.plugins.contains_key(&dep.name) {
                    self.visit_for_order(&dep.name, visited, order)?;
                }
            }
        }

        visited.insert(name.to_string(), true);
        order.push(name.to_string());

        Ok(())
    }
}
