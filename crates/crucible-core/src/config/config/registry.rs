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

/// The name the bundled help corpus is offered under.
///
/// Stated once because two rules read it: [`resolve_kiln_entries`] injects the
/// entry, and [`CliAppConfig::resolved_default_kiln`] falls back to it when a
/// config names no kiln at all. A literal in both places is how the fallback
/// ends up pointing at a name the map does not hold.
///
/// [`CliAppConfig::resolved_default_kiln`]: crate::config::CliAppConfig::resolved_default_kiln
pub const BUNDLED_DOCS_KILN: &str = "crucible-docs";

/// The entry in `kilns` whose key names `name`, ignoring ASCII case.
///
/// The `[kilns]` map is keyed by the raw string a user typed, and
/// [`KilnName`](crate::config::KilnName) resolves case-insensitively. Without
/// one folding lookup, a `default_kiln = "Crucible Help"` pointer misses a
/// `[kilns."crucible help"]` entry and the config layer answers a name the
/// daemon registry happily resolves — the same "the list offers a name the
/// attach refuses" shape, one layer up.
pub fn find_kiln_entry<'a, T>(
    kilns: &'a BTreeMap<String, T>,
    name: &str,
) -> Option<(&'a String, &'a T)> {
    if let Some((key, entry)) = kilns.get_key_value(name) {
        return Some((key, entry));
    }
    let folded = crate::config::KilnName::fold_str(name);
    kilns
        .iter()
        .find(|(key, _)| crate::config::KilnName::fold_str(key) == folded)
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
///
/// The synthesized `kiln_path` entry is lazy for a related but distinct
/// reason: a `[kilns]` entry is a kiln the user NAMED, and boot opens those,
/// while `kiln_path` defaults to whatever directory the client was standing in
/// when it spawned the daemon. See the test for why the two cannot be told
/// apart by provenance.
pub fn resolve_kiln_entries(
    kiln_path: &Path,
    kilns: &BTreeMap<String, KilnEntry>,
) -> BTreeMap<String, KilnEntry> {
    let mut map = if kilns.is_empty() {
        synthesized_kiln_name(kiln_path)
            .map(|name| {
                BTreeMap::from([(
                    name,
                    KilnEntry::Config {
                        path: kiln_path.to_path_buf(),
                        // LAZY, and the flag is the whole difference between a
                        // kiln the user named and a directory the daemon
                        // happened to start in. `kiln_path` defaults to the
                        // process working directory, so an eager entry here
                        // meant a daemon spawned in a source tree opened and
                        // indexed that source tree at boot. It stays
                        // registered — addressable by name, opened on first
                        // use — it simply does not open unasked.
                        lazy: true,
                        auto: false,
                    },
                )])
            })
            .unwrap_or_default()
    } else {
        kilns.clone()
    };

    if let Some(docs) = crate::bundled_docs::bundled_docs_dir() {
        map.entry(BUNDLED_DOCS_KILN.to_string())
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
    /// pointer. The directory's own spelling survives, capitals and space
    /// included — the name a user reads is the name of their folder.
    #[test]
    fn a_kiln_path_only_config_names_the_kiln_after_its_directory() {
        let map = resolve_kiln_entries(Path::new("/home/u/My Vault"), &BTreeMap::new());
        assert_eq!(map["My Vault"].path(), PathBuf::from("/home/u/My Vault"));
        assert!(
            !map.contains_key("default"),
            "\"default\" is a pointer, never a kiln's identity: {map:?}"
        );
        assert_eq!(
            synthesized_kiln_name(Path::new("~/vault")).as_deref(),
            Some("vault")
        );
    }

    /// The pointer and the key are two strings a user types by hand, and one
    /// of them carries the case. The lookup folds so the pair cannot disagree.
    #[test]
    fn a_kiln_entry_is_found_whatever_case_the_pointer_uses() {
        let kilns = BTreeMap::from([(
            "Crucible Help".to_string(),
            KilnEntry::Path(PathBuf::from("/docs")),
        )]);

        for pointer in ["Crucible Help", "crucible help", "CRUCIBLE HELP"] {
            let (key, entry) =
                find_kiln_entry(&kilns, pointer).expect("the entry must be found: {pointer}");
            assert_eq!(key, "Crucible Help", "the registered spelling comes back");
            assert_eq!(entry.path(), PathBuf::from("/docs"));
        }
        assert!(find_kiln_entry(&kilns, "other").is_none());
    }

    /// The synthesized entry is LAZY, and that is the difference between a
    /// kiln a user named and a directory the daemon happened to start in.
    ///
    /// `kiln_path` defaults to the process working directory, and the daemon
    /// is spawned by whichever client the user ran, wherever they stood. Boot
    /// opens every eager entry, so an eager synthesized entry meant that
    /// `cru web` in a source tree opened and indexed that source tree. The
    /// entry stays REGISTERED — it is addressable by name and opens on first
    /// use — it simply does not open unasked.
    ///
    /// The daemon cannot distinguish a `kiln_path` the user wrote from the
    /// default: provenance is `#[serde(skip)]`, so the value crosses the wire
    /// with the default already applied. Guessing which it was is the
    /// inference this registry exists to refuse, so both are lazy, and a user
    /// who wants the eager rule writes a `[kilns]` entry — which the config
    /// reference already recommends over `kiln_path`.
    #[test]
    fn a_synthesized_kiln_path_entry_is_lazy() {
        let map = resolve_kiln_entries(Path::new("/home/u/My Vault"), &BTreeMap::new());

        assert!(
            map["My Vault"].lazy(),
            "a directory the daemon merely started in must not open unasked: {map:?}"
        );
    }

    /// A kiln the user NAMED keeps the eager rule, so boot opens it and the
    /// kiln-addressed routes answer for it after a restart.
    #[test]
    fn a_declared_kilns_entry_stays_eager() {
        let declared = BTreeMap::from([(
            "vault".to_string(),
            KilnEntry::Path(PathBuf::from("/home/u/vault")),
        )]);

        let map = resolve_kiln_entries(Path::new("/home/u/elsewhere"), &declared);

        assert!(!map["vault"].lazy(), "{map:?}");
        assert!(
            !map.contains_key("elsewhere"),
            "a declared `[kilns]` table replaces the synthesis: {map:?}"
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
