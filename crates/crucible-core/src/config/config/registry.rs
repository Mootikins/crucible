use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A named kiln entry in global config.
///
/// Supports shorthand (just a path string) and full form (table with options).
/// Shorthand: `vault = "~/vault"`
/// Full: `[kilns.work]\npath = "~/work/notes"\nlazy = true`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KilnEntry {
    /// Shorthand: just a path string.
    Path(PathBuf),
    /// Full form: table with path and optional lazy flag.
    Config {
        /// Filesystem path to the kiln root.
        path: PathBuf,
        /// If true, kiln is not opened until explicitly requested.
        #[serde(default)]
        lazy: bool,
        /// True when Crucible wrote this entry itself, because a `--kiln`
        /// flag named a directory that had no entry yet.
        ///
        /// Nothing in Crucible reads it: it is a marker for the human whose
        /// config file grew a line they did not type, so they can tell their
        /// own entries from ours and delete ours without wondering what
        /// depends on it.
        ///
        /// Modelled rather than merely written, because a config that
        /// round-trips through serde drops what the struct does not describe.
        /// No writer does that any more — Crucible writes no `[kilns]` entries
        /// at all, and an entry here is one a user typed or one an older
        /// version left behind — but the field has to survive being READ and
        /// written back by anything that ever does.
        #[serde(default)]
        auto: bool,
    },
}

impl KilnEntry {
    /// Returns the filesystem path for this kiln entry.
    pub fn path(&self) -> PathBuf {
        match self {
            KilnEntry::Path(p) => p.clone(),
            KilnEntry::Config { path, .. } => path.clone(),
        }
    }

    /// Returns whether this kiln should be lazily opened.
    pub fn lazy(&self) -> bool {
        match self {
            KilnEntry::Path(_) => false,
            KilnEntry::Config { lazy, .. } => *lazy,
        }
    }
}

/// The name a `kiln_path`-only config gives its one kiln: the directory
/// basename, folded to the registry charset.
///
/// `None` when the path has no usable basename (`/`, `""`, `.`). The caller
/// then synthesizes no entry, because a name that resolves to nothing is the
/// shape the daemon registry refuses to produce. "default" is never the
/// answer here: it is the `default_kiln` *pointer*, and a kiln named by it
/// showed up as "default" in every picker while its directory said otherwise.
pub fn synthesized_kiln_name(kiln_path: &Path) -> Option<String> {
    kiln_path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(crate::config::KilnName::normalize)
        .map(|name| name.to_string())
}

/// The effective `[kilns]` map for a config's `kiln_path` + `[kilns]` pair.
///
/// The body of [`CliAppConfig::resolved_kilns`](crate::config::CliAppConfig::resolved_kilns),
/// lifted out so the daemon can build the same map from the config JSON it is
/// *handed* rather than re-reading the config file. Two answers to "which
/// kilns exist" is how the daemon ends up with an empty registry for the
/// shipped `kiln_path`-only config shape, and without the bundled
/// `crucible-docs` entry at all — it is injected here, never into `self.kilns`.
///
/// See that method for why `crucible-docs` must stay out of the stored map and
/// why it is `lazy`.
pub fn resolve_kiln_entries(
    kiln_path: &Path,
    kilns: &BTreeMap<String, KilnEntry>,
) -> BTreeMap<String, KilnEntry> {
    let mut map = if kilns.is_empty() {
        synthesized_kiln_name(kiln_path)
            .map(|name| BTreeMap::from([(name, KilnEntry::Path(kiln_path.to_path_buf()))]))
            .unwrap_or_default()
    } else {
        kilns.clone()
    };

    if let Some(docs) = crate::bundled_docs::bundled_docs_dir() {
        map.entry("crucible-docs".to_string())
            .or_insert(KilnEntry::Config {
                path: docs,
                lazy: true,
                auto: false,
            });
    }

    map
}

/// A registered project in global config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Filesystem path to the project root.
    pub path: PathBuf,
    /// Named kilns this project uses (resolved from `[kilns]` section).
    #[serde(default)]
    pub kilns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn kiln_entry_shorthand_deserializes_from_string() {
        let toml_str = r#"vault = "~/vault""#;
        let map: std::collections::BTreeMap<String, KilnEntry> = toml::from_str(toml_str).unwrap();
        assert_eq!(map["vault"].path(), PathBuf::from("~/vault"));
        assert!(!map["vault"].lazy());
    }

    #[test]
    fn kiln_entry_full_deserializes_from_table() {
        let toml_str = r#"
[work]
path = "~/work/notes"
lazy = true
"#;
        let map: std::collections::BTreeMap<String, KilnEntry> = toml::from_str(toml_str).unwrap();
        assert_eq!(map["work"].path(), PathBuf::from("~/work/notes"));
        assert!(map["work"].lazy());
    }

    #[test]
    fn project_entry_deserializes() {
        // `default_kiln` is a removed key. Old config files still contain
        // it, so the load must ignore it instead of an error.
        let toml_str = r#"
[crucible]
path = "~/crucible"
kilns = ["docs", "vault"]
default_kiln = "vault"
"#;
        let map: std::collections::HashMap<String, ProjectEntry> =
            toml::from_str(toml_str).unwrap();
        let entry = &map["crucible"];
        assert_eq!(entry.path, PathBuf::from("~/crucible"));
        assert_eq!(entry.kilns, vec!["docs", "vault"]);
    }

    #[test]
    fn kiln_entry_roundtrips_through_toml() {
        // TOML requires a table at the root, so roundtrip through a map
        // (matches real usage: `[kilns]` is always a table in config)
        let mut map = std::collections::HashMap::new();
        map.insert(
            "vault".to_string(),
            KilnEntry::Path(PathBuf::from("~/vault")),
        );
        let serialized = toml::to_string(&map).unwrap();
        let deserialized: std::collections::BTreeMap<String, KilnEntry> =
            toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized["vault"].path(), PathBuf::from("~/vault"));
    }

    /// `kiln_path` defaults to the working directory, so `cru --standalone
    /// web` run inside a kiln used to list that kiln as "default". The
    /// directory names the kiln; "default" is only ever the `default_kiln`
    /// pointer.
    #[test]
    fn a_kiln_path_only_config_names_the_kiln_after_its_directory() {
        let map = resolve_kiln_entries(Path::new("/home/u/My Vault"), &BTreeMap::new());
        assert_eq!(map["my-vault"].path(), PathBuf::from("/home/u/My Vault"));
        assert!(
            !map.contains_key("default"),
            "\"default\" is a pointer, never a kiln's identity: {map:?}"
        );
        assert_eq!(
            synthesized_kiln_name(Path::new("~/vault")).as_deref(),
            Some("vault")
        );
    }

    /// A path with no usable basename yields no entry rather than a name
    /// that resolves to nothing; the daemon floor refuses such a path anyway.
    #[test]
    fn a_kiln_path_with_no_usable_basename_synthesizes_no_entry() {
        for raw in ["/", "", "."] {
            let map = resolve_kiln_entries(Path::new(raw), &BTreeMap::new());
            assert!(
                map.keys().all(|k| k == "crucible-docs"),
                "{raw:?} must synthesize nothing: {map:?}"
            );
        }
    }
}
