//! Per-leaf provenance for the config store.
//!
//! [`ProvenanceMap`] records, for each dot-joined leaf path, which source
//! last wrote it. An array is one leaf: arrays replace wholesale, so
//! per-element provenance cannot exist.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a config value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTag {
    /// The compiled-in default.
    Default,
    /// The deprecated `config.toml` seed.
    Toml(std::path::PathBuf),
    /// A `cru.config.set` call (or the returned table) in a Lua file.
    Lua {
        /// The Lua chunk name, usually the file path.
        file: String,
        /// The call-site line; `None` for a returned table.
        line: Option<u32>,
    },
    /// A runtime `config.set` RPC merge.
    Rpc,
    /// A CLI flag override.
    Cli,
    /// The daemon state overlay (`kilns.json`, `llm.json`) — what the daemon
    /// was told through a registration surface.
    Registered,
    /// A value the daemon discovered on its own (for example a project
    /// matched from a directory walk).
    Discovered,
}

impl SourceTag {
    /// One-word source name for table rendering.
    pub fn short(&self) -> &'static str {
        match self {
            SourceTag::Default => "default",
            SourceTag::Toml(_) => "toml",
            SourceTag::Lua { .. } => "lua",
            SourceTag::Rpc => "rpc",
            SourceTag::Cli => "cli",
            SourceTag::Registered => "registered",
            SourceTag::Discovered => "discovered",
        }
    }

    /// The rendered detail, for `cru config show --sources`.
    pub fn detail(&self) -> String {
        match self {
            SourceTag::Default => "default".to_string(),
            SourceTag::Toml(path) => format!("toml ({})", path.display()),
            SourceTag::Lua {
                file,
                line: Some(line),
            } => format!("lua ({file}:{line})"),
            SourceTag::Lua { file, line: None } => format!("lua ({file})"),
            SourceTag::Rpc => "rpc".to_string(),
            SourceTag::Cli => "cli".to_string(),
            SourceTag::Registered => "registered".to_string(),
            SourceTag::Discovered => "discovered".to_string(),
        }
    }
}

/// Dot-joined leaf path → the source that last wrote it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvenanceMap {
    entries: BTreeMap<String, SourceTag>,
}

impl ProvenanceMap {
    /// An empty map. Every absent entry renders as `default`.
    pub fn new() -> Self {
        Self::default()
    }

    /// The source recorded for `path`, if any.
    pub fn get(&self, path: &str) -> Option<&SourceTag> {
        self.entries.get(path)
    }

    /// Record `tag` for one leaf path.
    pub fn set(&mut self, path: impl Into<String>, tag: SourceTag) {
        self.entries.insert(path.into(), tag);
    }

    /// Drop every entry at `prefix` or under it.
    ///
    /// A replacement clears first, then records the new leaves — otherwise a
    /// removed provider would keep a ghost provenance row.
    pub fn clear_prefix(&mut self, prefix: &str) {
        let child_prefix = format!("{prefix}.");
        self.entries
            .retain(|path, _| path != prefix && !path.starts_with(&child_prefix));
    }

    /// Iterate the recorded entries in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SourceTag)> {
        self.entries.iter()
    }

    /// The number of recorded leaves.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_prefix_keeps_a_sibling_that_shares_the_prefix_text() {
        let mut map = ProvenanceMap::new();
        map.set("llm.default", SourceTag::Rpc);
        map.set("llm.default_model", SourceTag::Cli);
        map.set("llm.default.endpoint", SourceTag::Rpc);
        map.clear_prefix("llm.default");
        assert!(map.get("llm.default").is_none());
        assert!(map.get("llm.default.endpoint").is_none());
        assert_eq!(map.get("llm.default_model"), Some(&SourceTag::Cli));
    }
}
