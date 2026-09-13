//! The spec: the operator's list of plugins, one entry per plugin.
//!
//! Data only. The one function an entry may carry, `config`, stays in the
//! Lua VM that defined it. This type is what `plugin.list`, the bootstrap
//! and the loader read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// `plugins.<name>.enabled`, the Builtin fragment (the installed manifest
    /// merges at the same rank), then `true`. A plugin's own `spec.luau`
    /// has no `enabled` field, so it never answers. `docs/Meta/CONTEXT.md`
    /// defines the terms.
    pub enabled: Option<bool>,
    /// The table passed to `setup(opts)`. Object or `Null`.
    #[serde(default)]
    pub opts: Value,
    /// Whether the defining VM holds a `config` function for this name.
    /// OR-merged across ranks, so `true` means some rank holds one; the
    /// store keyed by name says which.
    #[serde(default)]
    pub has_config: bool,
}

/// Extract a safe plugin directory name from a git URL.
///
/// The name is the URL's last segment without a trailing `.git`. It is
/// checked by the same rule the fragment reader applies to a plugin's own
/// name (`PluginManifest::validate` in crucible-lua): it starts with a
/// lowercase letter, holds only `[a-z0-9_-]`, is at most 64 bytes, and does
/// not end with `-` or `_`. One rule for both, so a spec entry the fragment
/// would refuse is refused before anything is cloned.
///
/// Returns `None` when the segment fails that rule. The rule also refuses
/// `.`, `..`, a leading `-` (a CLI flag to any tool the name later reaches)
/// and every shell metacharacter.
pub fn plugin_name_from_url(url: &str) -> Option<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");
    is_valid_plugin_name(name).then(|| name.to_string())
}

/// The plugin name rule, in words. Every refusal of a name quotes this
/// text, so the rule is stated once and read from one place.
pub const PLUGIN_NAME_RULE: &str = "a plugin name starts with a lowercase letter, holds only \
     a-z, 0-9, '-' and '_', is at most 64 bytes, and does not end with '-' or '_'";

/// The plugin name rule. One function for the URL-derived name, the
/// directory name and a fragment's declared name (`PluginManifest::validate`
/// in crucible-lua calls this one).
pub fn is_valid_plugin_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && !name.ends_with('-')
        && !name.ends_with('_')
}

/// Who wrote an entry. Higher wins. `enabled` resolves in this order, first
/// answer wins: the operator's entry, the config leaf
/// `plugins.<name>.enabled`, the Builtin fragment (the installed manifest
/// merges at the same rank), then `true`. A plugin's own `spec.luau` has no
/// `enabled` field, so it never answers. `Spec::merge` is the only reader of
/// the order, and `docs/Meta/CONTEXT.md` defines the terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecRank {
    /// A plugin's own `init.luau` calling `cru.plugin.setup` during
    /// activation. A fragment file (`spec.luau`) has no `cru` and never
    /// writes the spec: discovery reads it into the manifest instead.
    PluginFragment,
    /// The shipped defaults in `runtime/defaults/init.luau`.
    Builtin,
    /// The operator's `init.lua`.
    Operator,
}

/// The merged spec: one entry per plugin name, with the rank that wrote it.
///
/// Beside the merged entry, the spec keeps what each rank wrote on its own.
/// The `enabled` and `opts` resolution rules read one rank at a time: the
/// operator's `enabled` beats a settings leaf, a Builtin `enabled` loses to
/// it, and a settings leaf sits between the Builtin `opts` and the
/// operator's. A merged entry alone cannot say which rank wrote a field.
#[derive(Debug, Default, Clone)]
pub struct Spec {
    entries: BTreeMap<String, (SpecRank, SpecEntry)>,
    layers: BTreeMap<String, BTreeMap<SpecRank, SpecEntry>>,
}

impl SpecEntry {
    /// The positional slot: a bare name, `user/repo`, or a URL.
    pub fn from_positional(text: &str) -> Result<Self, String> {
        let text = text.trim();
        // `plugin_name_from_url` reads the last segment only, so `../x` would
        // pass as `x`. A relative path segment is neither a name nor a remote.
        let name = plugin_name_from_url(text)
            .filter(|_| !text.split('/').any(|seg| seg == "." || seg == ".."))
            .ok_or_else(|| format!("'{text}' does not name a plugin: {PLUGIN_NAME_RULE}"))?;
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
        match self.layers.entry(name.clone()).or_default().entry(rank) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(entry.clone());
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                slot.get_mut().absorb(entry.clone());
            }
        }
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

    /// What `rank` alone wrote for `name`: its own entries merged, and no
    /// other rank's. `None` when that rank never wrote to the name.
    pub fn at(&self, name: &str, rank: SpecRank) -> Option<&SpecEntry> {
        self.layers.get(name)?.get(&rank)
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

    /// The name is a directory on the runtimepath, and the fragment reader
    /// accepts only a lowercase start, `[a-z0-9_-]` and no trailing `-` or
    /// `_`. The spec applies the same rule, so an entry the fragment would
    /// refuse is refused here, with the reason.
    #[test]
    fn a_name_the_fragment_would_refuse_is_refused_here() {
        let err = SpecEntry::from_positional("MyPlugin").unwrap_err();
        assert!(err.contains("lowercase"), "{err}");
        assert!(SpecEntry::from_positional("user/My-Repo.git").is_err());
        assert!(SpecEntry::from_positional("my.plugin").is_err());
        assert!(SpecEntry::from_positional("plugin-").is_err());
        assert!(SpecEntry::from_positional("user/my_plugin-2.git").is_ok());
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

    /// Each rank keeps what it wrote, so a reader can ask one rank alone.
    #[test]
    fn each_rank_keeps_what_it_wrote() {
        let mut spec = Spec::default();
        spec.merge(
            SpecEntry {
                enabled: Some(false),
                opts: json!({ "a": 1 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Builtin,
        );
        spec.merge(
            SpecEntry {
                opts: json!({ "b": 2 }),
                ..SpecEntry::from_positional("x").unwrap()
            },
            SpecRank::Operator,
        );
        let operator = spec.at("x", SpecRank::Operator).unwrap();
        assert_eq!(operator.enabled, None);
        assert_eq!(operator.opts, json!({ "b": 2 }));
        let builtin = spec.at("x", SpecRank::Builtin).unwrap();
        assert_eq!(builtin.enabled, Some(false));
        assert_eq!(builtin.opts, json!({ "a": 1 }));
        assert!(spec.at("x", SpecRank::PluginFragment).is_none());
        assert_eq!(spec.get("x").unwrap().opts, json!({ "a": 1, "b": 2 }));
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
