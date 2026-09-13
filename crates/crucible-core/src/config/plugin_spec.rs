//! The spec: the operator's list of plugins, one entry per plugin.
//!
//! Data only. The two functions an entry may carry, `config` and `init`,
//! stay in the Lua VM that defined them. This type is what `plugin.list`,
//! the bootstrap and the loader read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::config::plugin_name_from_url;

/// Where a plugin comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpecSource {
    /// A directory already on the runtimepath: shipped or local.
    Runtimepath,
    /// Cloned at boot when the directory is missing.
    Git {
        /// The `user/repo` short form or the full clone URL, as written.
        url: String,
        /// The branch to check out. `None` is the remote's default.
        branch: Option<String>,
        /// A commit or tag to pin to. `None` follows the branch head.
        pin: Option<String>,
    },
}

/// One table in the spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecEntry {
    /// The plugin's directory name on the runtimepath.
    pub name: String,
    /// Where the plugin comes from.
    pub source: SpecSource,
    /// `None` means "this entry does not say". `enabled` resolves in this
    /// order, first answer wins: the operator's entry, the config leaf
    /// `plugins.<name>.enabled`, the Builtin fragment, the plugin's fragment,
    /// then `true`. `docs/Meta/CONTEXT.md` defines the terms.
    pub enabled: Option<bool>,
    /// The table passed to `setup(opts)`. Object or `Null`.
    #[serde(default)]
    pub opts: Value,
    /// Whether the defining VM holds a `config` function for this name.
    /// OR-merged across ranks, so `true` means some rank holds one; the
    /// store keyed by name says which.
    #[serde(default)]
    pub has_config: bool,
    /// Whether the defining VM holds an `init` function for this name.
    /// OR-merged across ranks, so `true` means some rank holds one; the
    /// store keyed by name says which.
    #[serde(default)]
    pub has_init: bool,
}

/// Who wrote an entry. Higher wins. `enabled` resolves in this order, first
/// answer wins: the operator's entry, the config leaf
/// `plugins.<name>.enabled`, the Builtin fragment, the plugin's fragment,
/// then `true`. `Spec::merge` is the only reader of the order, and
/// `docs/Meta/CONTEXT.md` defines the terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpecRank {
    /// The plugin's own `spec.luau`.
    PluginFragment,
    /// The shipped defaults in `runtime/defaults/init.luau`.
    Builtin,
    /// The operator's `init.lua`.
    Operator,
}

/// The merged spec: one entry per plugin name, with the rank that wrote it.
#[derive(Debug, Default, Clone)]
pub struct Spec {
    entries: BTreeMap<String, (SpecRank, SpecEntry)>,
}

impl SpecEntry {
    /// The positional slot: a bare name, `user/repo`, or a URL.
    pub fn from_positional(text: &str) -> Result<Self, String> {
        let text = text.trim();
        // `plugin_name_from_url` reads the last segment only, so `../x` would
        // pass as `x`. A relative path segment is neither a name nor a remote.
        let name = plugin_name_from_url(text)
            .filter(|_| !text.split('/').any(|seg| seg == "." || seg == ".."))
            .ok_or_else(|| format!("'{text}' does not name a plugin the filesystem can hold"))?;
        let source = if text.contains('/') || text.contains(':') {
            SpecSource::Git {
                url: text.to_string(),
                branch: None,
                pin: None,
            }
        } else {
            SpecSource::Runtimepath
        };
        Ok(Self::new(name, source))
    }

    fn new(name: String, source: SpecSource) -> Self {
        Self {
            name,
            source,
            enabled: None,
            opts: Value::Null,
            has_config: false,
            has_init: false,
        }
    }

    /// Lay `over` on top of `self`, field by field. A field `over` leaves
    /// unsaid keeps the value `self` holds.
    fn absorb(&mut self, over: SpecEntry) {
        // A bare name says nothing about source, so Runtimepath never
        // overwrites Git. Only an operator entry can name a Git source; the
        // Builtin fragment lists directories and a plugin's own fragment
        // carries no source.
        if over.source != SpecSource::Runtimepath {
            self.source = over.source;
        }
        if over.enabled.is_some() {
            self.enabled = over.enabled;
        }
        merge_opts(&mut self.opts, over.opts);
        self.has_config |= over.has_config;
        self.has_init |= over.has_init;
    }
}

/// A shallow object merge: keys of `over` overwrite keys of `base`. A `Null`
/// on either side yields the other side unchanged.
fn merge_opts(base: &mut Value, over: Value) {
    match (base, over) {
        (_, Value::Null) => {}
        (base @ Value::Null, over) => *base = over,
        (Value::Object(base), Value::Object(over)) => base.extend(over),
        (base, over) => *base = over,
    }
}

impl Spec {
    /// Merge one entry. A higher rank lays its fields over the stored entry;
    /// an equal rank does the same with the later value winning; a lower rank
    /// fills only what the stored entry left unsaid.
    pub fn merge(&mut self, entry: SpecEntry, rank: SpecRank) {
        let name = entry.name.clone();
        match self.entries.get_mut(&name) {
            None => {
                self.entries.insert(name, (rank, entry));
            }
            Some((stored_rank, stored)) if rank >= *stored_rank => {
                stored.absorb(entry);
                *stored_rank = rank;
            }
            Some((_, stored)) => {
                let mut under = entry;
                under.absorb(std::mem::replace(
                    stored,
                    SpecEntry::new(name, SpecSource::Runtimepath),
                ));
                *stored = under;
            }
        }
    }

    /// The entry for `name`, if any rank wrote one.
    pub fn get(&self, name: &str) -> Option<&SpecEntry> {
        self.entries.get(name).map(|(_, e)| e)
    }

    /// The highest rank that wrote to `name`.
    pub fn rank_of(&self, name: &str) -> Option<SpecRank> {
        self.entries.get(name).map(|(r, _)| *r)
    }

    /// Every entry, in name order.
    pub fn iter(&self) -> impl Iterator<Item = &SpecEntry> {
        self.entries.values().map(|(_, e)| e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bare_name_is_a_runtimepath_entry() {
        let entry = SpecEntry::from_positional("reflection").unwrap();
        assert_eq!(entry.name, "reflection");
        assert_eq!(entry.source, SpecSource::Runtimepath);
    }

    #[test]
    fn a_slash_or_a_url_is_a_git_entry_named_by_its_last_segment() {
        let short = SpecEntry::from_positional("user/greeter").unwrap();
        assert_eq!(short.name, "greeter");
        assert_eq!(
            short.source,
            SpecSource::Git {
                url: "user/greeter".into(),
                branch: None,
                pin: None
            }
        );
        let long = SpecEntry::from_positional("https://github.com/user/greeter.git").unwrap();
        assert_eq!(long.name, "greeter");
    }

    #[test]
    fn a_name_the_filesystem_cannot_hold_is_refused() {
        assert!(SpecEntry::from_positional("../x").is_err());
        assert!(SpecEntry::from_positional("-x").is_err());
        assert!(SpecEntry::from_positional("").is_err());
    }

    #[test]
    fn a_later_entry_for_the_same_name_merges_over_an_earlier_one() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: Some(true),
                opts: json!({ "a": 1, "b": 1 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::PluginFragment,
        );
        spec.merge(
            SpecEntry {
                enabled: Some(false),
                opts: json!({ "b": 2 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        let x = spec.get("x").unwrap();
        assert_eq!(x.enabled, Some(false));
        assert_eq!(x.opts, json!({ "a": 1, "b": 2 }));
    }

    #[test]
    fn a_lower_rank_never_overrides_a_higher_one() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: Some(false),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        spec.merge(
            SpecEntry {
                enabled: Some(true),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Builtin,
        );
        assert_eq!(spec.get("x").unwrap().enabled, Some(false));
    }

    #[test]
    fn a_lower_rank_fills_only_what_the_stored_entry_left_unsaid() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: None,
                opts: json!({ "a": 1 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        spec.merge(
            SpecEntry {
                enabled: Some(true),
                opts: json!({ "a": 2, "b": 2 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Builtin,
        );
        let x = spec.get("x").unwrap();
        assert_eq!(x.enabled, Some(true));
        assert_eq!(x.opts, json!({ "a": 1, "b": 2 }));
        assert_eq!(spec.rank_of("x"), Some(SpecRank::Operator));
    }

    #[test]
    fn a_bare_name_never_erases_a_git_source_at_equal_rank() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry::from_positional("user/greeter").unwrap(),
            SpecRank::Operator,
        );
        spec.merge(
            SpecEntry {
                opts: json!({ "n": 1 }),
                ..SpecEntry::from_positional("greeter").unwrap()
            },
            SpecRank::Operator,
        );
        let greeter = spec.get("greeter").unwrap();
        assert!(
            matches!(&greeter.source, SpecSource::Git { url, .. } if url == "user/greeter"),
            "source was {:?}",
            greeter.source
        );
        assert_eq!(greeter.opts, json!({ "n": 1 }));
    }
}
